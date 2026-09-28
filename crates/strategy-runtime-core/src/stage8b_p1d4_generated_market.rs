//! Stage 8B-P1-d4 generated-Market publication identity and package composition.
//!
//! This module is broker-neutral and transport-free. It defines the exact
//! precommitted Redis identity bytes carried by the authenticated Stage 5G
//! package, but owns no Redis connection, filesystem handle, provider, source
//! acknowledgement, or live execution capability.

use broker_core::StrategyRequestId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stage5g_order_position::Stage5gOrderPositionState;
use crate::stage5g_p1_semantic::Stage5gP1SemanticCommitProjectionV1;
use crate::stage8b_p1d2_market_feedback::Stage8bP1d2MarketFeedbackProjectionV1;
use crate::stage8b_p1d3_working_limit::Stage8bP1d3ReplacementProjectionV1;
use crate::{
    export_stage5g_clean_restart, restore_stage5g_clean_restart, Stage5gCleanRestartExportInput,
    Stage5gCleanRestartSource, Stage5gCleanRestartedCapability, Stage5gLifecycleCommitmentKey,
};

pub const STAGE8B_P1D4_RESERVATION_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1D4_BINDING_SCHEMA_VERSION: u16 = 1;
pub const STAGE8B_P1D4_COMPOSITION_SCHEMA_VERSION: u16 = 1;

const RESERVATION_DOMAIN: &str = "moex.stage8b.p1d4.command-publication-reservation.v1";
const RESERVATION_CANONICAL_DOMAIN: &[u8] =
    b"moex.stage8b.p1d4.command-publication-reservation.canonical.v1\0";
const BINDING_DOMAIN: &str = "moex.stage8b.p1d4.command-publication-binding.v1";
const BINDING_CANONICAL_DOMAIN: &[u8] =
    b"moex.stage8b.p1d4.command-publication-binding.canonical.v1\0";
const COMPOSITION_DOMAIN: &str = "moex.stage8b.p1d4.generated-market-composition.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage8bP1d4GeneratedMarketPhase {
    Prepublication,
    AckCommitted,
    TruthCommitted,
}

/// Authenticated, read-only discriminator used by Stage 7 restart routing.
///
/// The value carries publication identity only; it grants no Redis, provider,
/// ACK, truth or source-resolution capability. `None` at the caller means the
/// P1-d4 discriminator was absent and preserves the accepted standalone
/// P1-d2 path. A declared but invalid composition never produces this value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage8bP1d4GeneratedMarketPackageState {
    Prepublication {
        reservation: Stage8bP1d4CommandPublicationReservationV1,
    },
    AckCommitted {
        reservation: Stage8bP1d4CommandPublicationReservationV1,
        binding: Stage8bP1d4CommandPublicationBindingV1,
    },
    TruthCommitted {
        reservation: Stage8bP1d4CommandPublicationReservationV1,
        binding: Stage8bP1d4CommandPublicationBindingV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1d4CommandPublicationReservationV1 {
    schema_version: u16,
    domain: String,
    source_stream: String,
    source_group: String,
    source_m10_redis_id: String,
    semantic_batch_id_sha256: String,
    strategy_request_id: StrategyRequestId,
    canonical_command_sha256: String,
    canonical_envelope_sha256: String,
    command_stream: String,
    command_group: String,
    command_stream_predecessor_id: String,
    reserved_command_entry_id: String,
    prepublication_package_generation: u64,
    publication_reservation_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1d4CommandPublicationBindingV1 {
    schema_version: u16,
    domain: String,
    source_stream: String,
    source_group: String,
    source_m10_redis_id: String,
    semantic_batch_id_sha256: String,
    strategy_request_id: StrategyRequestId,
    canonical_command_sha256: String,
    canonical_envelope_sha256: String,
    command_stream: String,
    command_group: String,
    command_stream_predecessor_id: String,
    command_entry_id: String,
    prepublication_package_generation: u64,
    publication_reservation_sha256: String,
    prepublication_seal_generation: u64,
    prepublication_seal_commitment_sha256: String,
    publication_binding_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage8bP1d4GeneratedMarketCompositionV1 {
    schema_version: u16,
    domain: String,
    phase: Stage8bP1d4GeneratedMarketPhase,
    reservation: Stage8bP1d4CommandPublicationReservationV1,
    binding: Option<Stage8bP1d4CommandPublicationBindingV1>,
}

/// Linear Stage 5G export source. All fields are committed in one package, so
/// P1-d3 working-book authority cannot be detached from generated-Market
/// publication or feedback authority.
pub struct Stage8bP1d4RestartSource {
    runtime: crate::HybridIntradayRuntimeStrategy,
    state: Stage5gOrderPositionState,
    p1d3: Stage8bP1d3ReplacementProjectionV1,
    semantic: Stage5gP1SemanticCommitProjectionV1,
    feedback: Option<Stage8bP1d2MarketFeedbackProjectionV1>,
    composition: Stage8bP1d4GeneratedMarketCompositionV1,
}

pub(crate) struct Stage8bP1d4AckStageResult {
    pub(crate) restored: Stage5gCleanRestartedCapability,
    pub(crate) restart_package: Vec<u8>,
}

pub(crate) struct Stage8bP1d4TruthStageResult {
    pub(crate) restored: Stage5gCleanRestartedCapability,
    pub(crate) restart_package: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1d4GeneratedMarketError {
    #[error("P1-d4 publication identity is invalid")]
    InvalidIdentity,
    #[error("P1-d4 canonical encoding is invalid")]
    InvalidEncoding,
    #[error("P1-d4 generation relation is invalid")]
    InvalidGeneration,
    #[error("P1-d4 composite package is inconsistent")]
    InvalidComposition,
}

impl Stage8bP1d4CommandPublicationReservationV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_stream: String,
        source_group: String,
        source_m10_redis_id: String,
        semantic_batch_id_sha256: String,
        strategy_request_id: StrategyRequestId,
        canonical_command_sha256: String,
        canonical_envelope_sha256: String,
        command_stream: String,
        command_group: String,
        command_stream_predecessor_id: String,
        reserved_command_entry_id: String,
        prepublication_package_generation: u64,
    ) -> Result<Self, Stage8bP1d4GeneratedMarketError> {
        let mut value = Self {
            schema_version: STAGE8B_P1D4_RESERVATION_SCHEMA_VERSION,
            domain: RESERVATION_DOMAIN.to_string(),
            source_stream,
            source_group,
            source_m10_redis_id,
            semantic_batch_id_sha256,
            strategy_request_id,
            canonical_command_sha256,
            canonical_envelope_sha256,
            command_stream,
            command_group,
            command_stream_predecessor_id,
            reserved_command_entry_id,
            prepublication_package_generation,
            publication_reservation_sha256: String::new(),
        };
        value.publication_reservation_sha256 = sha256_hex(&value.canonical_bytes_unchecked()?);
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), Stage8bP1d4GeneratedMarketError> {
        if self.schema_version != STAGE8B_P1D4_RESERVATION_SCHEMA_VERSION
            || self.domain != RESERVATION_DOMAIN
            || self.prepublication_package_generation == 0
            || !token(&self.source_stream)
            || !token(&self.source_group)
            || !token(&self.command_stream)
            || !token(&self.command_group)
            || !is_sha256(&self.semantic_batch_id_sha256)
            || !is_sha256(&self.canonical_command_sha256)
            || !is_sha256(&self.canonical_envelope_sha256)
            || !is_sha256(&self.publication_reservation_sha256)
        {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidIdentity);
        }
        let predecessor = parse_redis_id(&self.command_stream_predecessor_id)?;
        let reserved = parse_redis_id(&self.reserved_command_entry_id)?;
        if immediate_successor(predecessor)? != reserved
            || parse_redis_id(&self.source_m10_redis_id).is_err()
            || sha256_hex(&self.canonical_bytes_unchecked()?) != self.publication_reservation_sha256
        {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidIdentity);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, Stage8bP1d4GeneratedMarketError> {
        self.validate()?;
        self.canonical_bytes_unchecked()
    }

    fn canonical_bytes_unchecked(&self) -> Result<Vec<u8>, Stage8bP1d4GeneratedMarketError> {
        let mut body = Vec::new();
        body.extend_from_slice(&self.schema_version.to_be_bytes());
        lp_utf8(&mut body, &self.domain)?;
        lp_utf8(&mut body, &self.source_stream)?;
        lp_utf8(&mut body, &self.source_group)?;
        redis_id_bytes(&mut body, &self.source_m10_redis_id)?;
        raw_sha256(&mut body, &self.semantic_batch_id_sha256)?;
        body.extend_from_slice(&uuid_bytes(self.strategy_request_id));
        raw_sha256(&mut body, &self.canonical_command_sha256)?;
        raw_sha256(&mut body, &self.canonical_envelope_sha256)?;
        lp_utf8(&mut body, &self.command_stream)?;
        lp_utf8(&mut body, &self.command_group)?;
        redis_id_bytes(&mut body, &self.command_stream_predecessor_id)?;
        redis_id_bytes(&mut body, &self.reserved_command_entry_id)?;
        body.extend_from_slice(&self.prepublication_package_generation.to_be_bytes());
        framed(RESERVATION_CANONICAL_DOMAIN, body)
    }

    pub fn source_stream(&self) -> &str {
        &self.source_stream
    }
    pub fn source_group(&self) -> &str {
        &self.source_group
    }
    pub fn source_m10_redis_id(&self) -> &str {
        &self.source_m10_redis_id
    }
    pub fn semantic_batch_id_sha256(&self) -> &str {
        &self.semantic_batch_id_sha256
    }
    pub fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }
    pub fn canonical_command_sha256(&self) -> &str {
        &self.canonical_command_sha256
    }
    pub fn canonical_envelope_sha256(&self) -> &str {
        &self.canonical_envelope_sha256
    }
    pub fn command_stream(&self) -> &str {
        &self.command_stream
    }
    pub fn command_group(&self) -> &str {
        &self.command_group
    }
    pub fn command_stream_predecessor_id(&self) -> &str {
        &self.command_stream_predecessor_id
    }
    pub fn reserved_command_entry_id(&self) -> &str {
        &self.reserved_command_entry_id
    }
    pub fn prepublication_package_generation(&self) -> u64 {
        self.prepublication_package_generation
    }
    pub fn publication_reservation_sha256(&self) -> &str {
        &self.publication_reservation_sha256
    }
}

impl Stage8bP1d4CommandPublicationBindingV1 {
    pub fn from_reservation(
        reservation: &Stage8bP1d4CommandPublicationReservationV1,
        prepublication_seal_generation: u64,
        prepublication_seal_commitment_sha256: String,
    ) -> Result<Self, Stage8bP1d4GeneratedMarketError> {
        reservation.validate()?;
        let mut value = Self {
            schema_version: STAGE8B_P1D4_BINDING_SCHEMA_VERSION,
            domain: BINDING_DOMAIN.to_string(),
            source_stream: reservation.source_stream.clone(),
            source_group: reservation.source_group.clone(),
            source_m10_redis_id: reservation.source_m10_redis_id.clone(),
            semantic_batch_id_sha256: reservation.semantic_batch_id_sha256.clone(),
            strategy_request_id: reservation.strategy_request_id,
            canonical_command_sha256: reservation.canonical_command_sha256.clone(),
            canonical_envelope_sha256: reservation.canonical_envelope_sha256.clone(),
            command_stream: reservation.command_stream.clone(),
            command_group: reservation.command_group.clone(),
            command_stream_predecessor_id: reservation.command_stream_predecessor_id.clone(),
            command_entry_id: reservation.reserved_command_entry_id.clone(),
            prepublication_package_generation: reservation.prepublication_package_generation,
            publication_reservation_sha256: reservation.publication_reservation_sha256.clone(),
            prepublication_seal_generation,
            prepublication_seal_commitment_sha256,
            publication_binding_sha256: String::new(),
        };
        value.publication_binding_sha256 = sha256_hex(&value.canonical_bytes_unchecked()?);
        value.validate_against(reservation)?;
        Ok(value)
    }

    pub fn validate_against(
        &self,
        reservation: &Stage8bP1d4CommandPublicationReservationV1,
    ) -> Result<(), Stage8bP1d4GeneratedMarketError> {
        reservation.validate()?;
        if self.schema_version != STAGE8B_P1D4_BINDING_SCHEMA_VERSION
            || self.domain != BINDING_DOMAIN
            || self.source_stream != reservation.source_stream
            || self.source_group != reservation.source_group
            || self.source_m10_redis_id != reservation.source_m10_redis_id
            || self.semantic_batch_id_sha256 != reservation.semantic_batch_id_sha256
            || self.strategy_request_id != reservation.strategy_request_id
            || self.canonical_command_sha256 != reservation.canonical_command_sha256
            || self.canonical_envelope_sha256 != reservation.canonical_envelope_sha256
            || self.command_stream != reservation.command_stream
            || self.command_group != reservation.command_group
            || self.command_stream_predecessor_id != reservation.command_stream_predecessor_id
            || self.command_entry_id != reservation.reserved_command_entry_id
            || self.prepublication_package_generation
                != reservation.prepublication_package_generation
            || self.publication_reservation_sha256 != reservation.publication_reservation_sha256
            || self.prepublication_seal_generation == 0
            || !is_sha256(&self.prepublication_seal_commitment_sha256)
            || !is_sha256(&self.publication_binding_sha256)
            || sha256_hex(&self.canonical_bytes_unchecked()?) != self.publication_binding_sha256
        {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidIdentity);
        }
        Ok(())
    }

    pub fn canonical_bytes(
        &self,
        reservation: &Stage8bP1d4CommandPublicationReservationV1,
    ) -> Result<Vec<u8>, Stage8bP1d4GeneratedMarketError> {
        self.validate_against(reservation)?;
        self.canonical_bytes_unchecked()
    }

    fn canonical_bytes_unchecked(&self) -> Result<Vec<u8>, Stage8bP1d4GeneratedMarketError> {
        let mut body = Vec::new();
        body.extend_from_slice(&self.schema_version.to_be_bytes());
        lp_utf8(&mut body, &self.domain)?;
        lp_utf8(&mut body, &self.source_stream)?;
        lp_utf8(&mut body, &self.source_group)?;
        redis_id_bytes(&mut body, &self.source_m10_redis_id)?;
        raw_sha256(&mut body, &self.semantic_batch_id_sha256)?;
        body.extend_from_slice(&uuid_bytes(self.strategy_request_id));
        raw_sha256(&mut body, &self.canonical_command_sha256)?;
        raw_sha256(&mut body, &self.canonical_envelope_sha256)?;
        lp_utf8(&mut body, &self.command_stream)?;
        lp_utf8(&mut body, &self.command_group)?;
        redis_id_bytes(&mut body, &self.command_stream_predecessor_id)?;
        redis_id_bytes(&mut body, &self.command_entry_id)?;
        body.extend_from_slice(&self.prepublication_package_generation.to_be_bytes());
        raw_sha256(&mut body, &self.publication_reservation_sha256)?;
        body.extend_from_slice(&self.prepublication_seal_generation.to_be_bytes());
        raw_sha256(&mut body, &self.prepublication_seal_commitment_sha256)?;
        framed(BINDING_CANONICAL_DOMAIN, body)
    }

    pub fn prepublication_package_generation(&self) -> u64 {
        self.prepublication_package_generation
    }
    pub fn prepublication_seal_generation(&self) -> u64 {
        self.prepublication_seal_generation
    }
    pub fn prepublication_seal_commitment_sha256(&self) -> &str {
        &self.prepublication_seal_commitment_sha256
    }
    pub fn publication_binding_sha256(&self) -> &str {
        &self.publication_binding_sha256
    }
    pub fn command_entry_id(&self) -> &str {
        &self.command_entry_id
    }
}

impl Stage8bP1d4GeneratedMarketCompositionV1 {
    pub(crate) fn prepublication(
        reservation: Stage8bP1d4CommandPublicationReservationV1,
    ) -> Result<Self, Stage8bP1d4GeneratedMarketError> {
        let value = Self {
            schema_version: STAGE8B_P1D4_COMPOSITION_SCHEMA_VERSION,
            domain: COMPOSITION_DOMAIN.to_string(),
            phase: Stage8bP1d4GeneratedMarketPhase::Prepublication,
            reservation,
            binding: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn ack_committed(
        reservation: Stage8bP1d4CommandPublicationReservationV1,
        binding: Stage8bP1d4CommandPublicationBindingV1,
    ) -> Result<Self, Stage8bP1d4GeneratedMarketError> {
        let value = Self {
            schema_version: STAGE8B_P1D4_COMPOSITION_SCHEMA_VERSION,
            domain: COMPOSITION_DOMAIN.to_string(),
            phase: Stage8bP1d4GeneratedMarketPhase::AckCommitted,
            reservation,
            binding: Some(binding),
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn into_truth_committed(mut self) -> Result<Self, Stage8bP1d4GeneratedMarketError> {
        self.validate()?;
        if self.phase != Stage8bP1d4GeneratedMarketPhase::AckCommitted {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
        }
        self.phase = Stage8bP1d4GeneratedMarketPhase::TruthCommitted;
        self.validate()?;
        Ok(self)
    }

    pub(crate) fn validate(&self) -> Result<(), Stage8bP1d4GeneratedMarketError> {
        if self.schema_version != STAGE8B_P1D4_COMPOSITION_SCHEMA_VERSION
            || self.domain != COMPOSITION_DOMAIN
        {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
        }
        self.reservation.validate()?;
        match (self.phase, self.binding.as_ref()) {
            (Stage8bP1d4GeneratedMarketPhase::Prepublication, None) => Ok(()),
            (
                Stage8bP1d4GeneratedMarketPhase::AckCommitted
                | Stage8bP1d4GeneratedMarketPhase::TruthCommitted,
                Some(binding),
            ) => binding.validate_against(&self.reservation),
            _ => Err(Stage8bP1d4GeneratedMarketError::InvalidComposition),
        }
    }

    pub(crate) fn phase(&self) -> Stage8bP1d4GeneratedMarketPhase {
        self.phase
    }
    pub(crate) fn reservation(&self) -> &Stage8bP1d4CommandPublicationReservationV1 {
        &self.reservation
    }
    pub(crate) fn binding(&self) -> Option<&Stage8bP1d4CommandPublicationBindingV1> {
        self.binding.as_ref()
    }

    pub(crate) fn package_state(
        &self,
    ) -> Result<Stage8bP1d4GeneratedMarketPackageState, Stage8bP1d4GeneratedMarketError> {
        self.validate()?;
        match self.phase {
            Stage8bP1d4GeneratedMarketPhase::Prepublication => {
                Ok(Stage8bP1d4GeneratedMarketPackageState::Prepublication {
                    reservation: self.reservation.clone(),
                })
            }
            Stage8bP1d4GeneratedMarketPhase::AckCommitted => {
                Ok(Stage8bP1d4GeneratedMarketPackageState::AckCommitted {
                    reservation: self.reservation.clone(),
                    binding: self
                        .binding
                        .clone()
                        .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?,
                })
            }
            Stage8bP1d4GeneratedMarketPhase::TruthCommitted => {
                Ok(Stage8bP1d4GeneratedMarketPackageState::TruthCommitted {
                    reservation: self.reservation.clone(),
                    binding: self
                        .binding
                        .clone()
                        .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?,
                })
            }
        }
    }

    pub(crate) fn validate_against_package(
        &self,
        package_write_generation: u64,
        state: &Stage5gOrderPositionState,
        p1d3: &Stage8bP1d3ReplacementProjectionV1,
        semantic: &Stage5gP1SemanticCommitProjectionV1,
        feedback: Option<&Stage8bP1d2MarketFeedbackProjectionV1>,
    ) -> Result<(), Stage8bP1d4GeneratedMarketError> {
        self.validate()?;
        p1d3.validate()
            .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
        if !semantic.validate() || semantic.intent_count != 1 {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
        }
        let request_id = semantic
            .request_id
            .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
        if self.reservation.source_m10_redis_id() != semantic.m10_redis_id
            || self.reservation.semantic_batch_id_sha256() != semantic.semantic_batch_id_sha256
            || self.reservation.strategy_request_id() != request_id
            || semantic.canonical_command_sha256.as_deref()
                != Some(self.reservation.canonical_command_sha256())
        {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
        }
        let expected_generation = match self.phase {
            Stage8bP1d4GeneratedMarketPhase::Prepublication => {
                if feedback.is_some() {
                    return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
                }
                self.reservation.prepublication_package_generation()
            }
            Stage8bP1d4GeneratedMarketPhase::AckCommitted => {
                let feedback = feedback
                    .filter(|value| {
                        value.phase()
                            == crate::stage8b_p1d2_market_feedback::Stage8bP1d2FeedbackPhase::AckCommitted
                            && value.request_id() == request_id
                            && crate::stage5g_order_position::stage8b_p1d4_ack_state_matches(
                                state,
                                &value.ack,
                                value.seq_ack(),
                            )
                            && crate::stage5g_order_position::stage8b_p1d4_truth_sequence_from_ack_state(
                                state,
                                request_id,
                            ) == Some(value.seq_truth())
                    })
                    .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
                let _ = feedback;
                self.reservation
                    .prepublication_package_generation()
                    .checked_add(1)
                    .ok_or(Stage8bP1d4GeneratedMarketError::InvalidGeneration)?
            }
            Stage8bP1d4GeneratedMarketPhase::TruthCommitted => {
                feedback
                    .filter(|value| {
                        value.phase()
                            == crate::stage8b_p1d2_market_feedback::Stage8bP1d2FeedbackPhase::TruthCommitted
                            && value.request_id() == request_id
                            && crate::stage5g_order_position::stage8b_p1d4_truth_state_matches(
                                state,
                                request_id,
                                value.seq_ack(),
                                value.seq_truth(),
                                &value.truth,
                            )
                    })
                    .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
                self.reservation
                    .prepublication_package_generation()
                    .checked_add(2)
                    .ok_or(Stage8bP1d4GeneratedMarketError::InvalidGeneration)?
            }
        };
        if package_write_generation != expected_generation {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidGeneration);
        }
        let (_, account_id, instrument) =
            crate::Stage5gOrderPositionSession::stage5g_restart_state_binding(state);
        if account_id != p1d3.working_book().account_id()
            || instrument != p1d3.working_book().instrument()
        {
            return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
        }
        Ok(())
    }
}

impl Stage8bP1d4RestartSource {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        runtime: crate::HybridIntradayRuntimeStrategy,
        state: Stage5gOrderPositionState,
        p1d3: Stage8bP1d3ReplacementProjectionV1,
        semantic: Stage5gP1SemanticCommitProjectionV1,
        feedback: Option<Stage8bP1d2MarketFeedbackProjectionV1>,
        composition: Stage8bP1d4GeneratedMarketCompositionV1,
    ) -> Result<Self, Stage8bP1d4GeneratedMarketError> {
        composition.validate()?;
        Ok(Self {
            runtime,
            state,
            p1d3,
            semantic,
            feedback,
            composition,
        })
    }

    pub(crate) fn runtime(&self) -> &crate::HybridIntradayRuntimeStrategy {
        &self.runtime
    }
    pub(crate) fn binding(
        &self,
    ) -> (
        &str,
        &broker_core::BrokerAccountId,
        &broker_core::InstrumentId,
    ) {
        crate::Stage5gOrderPositionSession::stage5g_restart_state_binding(&self.state)
    }
    pub(crate) fn summary(&self) -> crate::Stage5gOrderPositionSummary {
        crate::Stage5gOrderPositionSession::stage5g_restart_summary_from_state(&self.state, 1)
    }
    pub(crate) fn checkpoint(&self) -> crate::Stage5gTimerCheckpointEnvelope {
        crate::Stage5gOrderPositionSession::stage5g_restart_checkpoint_from_state(&self.state)
    }
    pub(crate) fn state(&self) -> Stage5gOrderPositionState {
        self.state.clone()
    }
    pub(crate) fn p1d3(&self) -> &Stage8bP1d3ReplacementProjectionV1 {
        &self.p1d3
    }
    pub(crate) fn semantic(&self) -> &Stage5gP1SemanticCommitProjectionV1 {
        &self.semantic
    }
    pub(crate) fn feedback(&self) -> Option<&Stage8bP1d2MarketFeedbackProjectionV1> {
        self.feedback.as_ref()
    }
    pub(crate) fn composition(&self) -> &Stage8bP1d4GeneratedMarketCompositionV1 {
        &self.composition
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_stage8b_p1d4_ack_stage(
    runtime: crate::HybridIntradayRuntimeStrategy,
    state: Stage5gOrderPositionState,
    p1d3: Stage8bP1d3ReplacementProjectionV1,
    semantic: Stage5gP1SemanticCommitProjectionV1,
    composition: Stage8bP1d4GeneratedMarketCompositionV1,
    binding: Stage8bP1d4CommandPublicationBindingV1,
    finalized: crate::stage8b_p1d2_market_feedback::Stage8bP1d2FinalizedMarketFeedbackInput,
    export_input: Stage5gCleanRestartExportInput,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d4AckStageResult, Stage8bP1d4GeneratedMarketError> {
    composition.validate()?;
    if composition.phase() != Stage8bP1d4GeneratedMarketPhase::Prepublication
        || composition.binding().is_some()
        || binding.validate_against(composition.reservation()).is_err()
        || export_input.write_generation
            != composition
                .reservation()
                .prepublication_package_generation()
                .checked_add(1)
                .ok_or(Stage8bP1d4GeneratedMarketError::InvalidGeneration)?
    {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
    }
    let (pre_position_qty, _) =
        crate::Stage5gOrderPositionSession::stage8b_p1d3_position_basis(&state)
            .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    let mut feedback = finalized.into_projection();
    let order = feedback
        .truth
        .orders
        .first()
        .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    let seq_ack = crate::Stage5gOrderPositionSession::stage8b_p1d3_total_sequence_frontier(&state)
        .checked_add(1)
        .ok_or(Stage8bP1d4GeneratedMarketError::InvalidGeneration)?;
    let seq_truth = seq_ack
        .checked_add(1)
        .ok_or(Stage8bP1d4GeneratedMarketError::InvalidGeneration)?;
    feedback.seq_ack = seq_ack;
    feedback.seq_truth = seq_truth;
    feedback.feedback_projection_sha256 =
        crate::stage8b_p1d2_market_feedback::feedback_projection_sha256(&feedback);
    if !feedback.validate() {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
    }
    crate::stage8b_p1d2_market_feedback::stage8b_p1d2_test_record_sequence_pair_before_crash(
        &["p1d4-generated-market-gm08", "p1d4-generated-market-gm09"],
        seq_ack,
        seq_truth,
    );
    crate::stage6d_live_core::stage8b_p1d4_test_crash_frontier("GM08");
    crate::stage8b_p1d2_market_feedback::stage8b_p1d2_test_crash_barrier(
        "p1d2-after-sequence-pair-before-ack",
    );
    let state = crate::stage5g_order_position::stage8b_p1d3_append_ack_state(
        state,
        &feedback.ack,
        seq_ack,
        crate::stage5g_order_position::Stage8bP1d3AckStateKind::MarketPlace {
            side: order.side,
            qty: order.qty,
            pre_position_qty,
            attribution: feedback.expected_attribution.clone(),
            source_event_ts_utc: feedback.source_ts_utc_ms.div_euclid(1_000),
        },
    )
    .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    if !crate::stage5g_order_position::stage8b_p1d4_ack_state_matches(
        &state,
        &feedback.ack,
        seq_ack,
    ) || crate::stage5g_order_position::stage8b_p1d4_truth_sequence_from_ack_state(
        &state,
        feedback.request_id,
    ) != Some(seq_truth)
    {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
    }
    crate::stage6d_live_core::stage8b_p1d4_test_crash_frontier("GM09");
    let composition = Stage8bP1d4GeneratedMarketCompositionV1::ack_committed(
        composition.reservation().clone(),
        binding,
    )?;
    let fresh_runtime = runtime.stage5g_clean_reconstruction_candidate();
    let source = Stage8bP1d4RestartSource::new(
        runtime,
        state,
        p1d3,
        semantic,
        Some(feedback),
        composition.clone(),
    )?;
    let restart_package = export_stage5g_clean_restart(
        Stage5gCleanRestartSource::P1d4(Box::new(source)),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    let restored = restore_stage5g_clean_restart(&restart_package, commitment_key, fresh_runtime)
        .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    if restored.stage8b_p1d4_generated_market_composition() != Some(&composition) {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
    }
    Ok(Stage8bP1d4AckStageResult {
        restored,
        restart_package,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_stage8b_p1d4_truth_stage(
    runtime: crate::HybridIntradayRuntimeStrategy,
    state: Stage5gOrderPositionState,
    p1d3: Stage8bP1d3ReplacementProjectionV1,
    semantic: Stage5gP1SemanticCommitProjectionV1,
    mut feedback: Stage8bP1d2MarketFeedbackProjectionV1,
    composition: Stage8bP1d4GeneratedMarketCompositionV1,
    export_input: Stage5gCleanRestartExportInput,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<Stage8bP1d4TruthStageResult, Stage8bP1d4GeneratedMarketError> {
    composition.validate()?;
    let binding = composition
        .binding()
        .ok_or(Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    if composition.phase() != Stage8bP1d4GeneratedMarketPhase::AckCommitted
        || binding.validate_against(composition.reservation()).is_err()
        || export_input.write_generation
            != composition
                .reservation()
                .prepublication_package_generation()
                .checked_add(2)
                .ok_or(Stage8bP1d4GeneratedMarketError::InvalidGeneration)?
    {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
    }
    let seq_truth = crate::stage5g_order_position::stage8b_p1d4_truth_sequence_from_ack_state(
        &state,
        feedback.request_id,
    )
    .ok_or(Stage8bP1d4GeneratedMarketError::InvalidGeneration)?;
    if seq_truth != feedback.seq_truth {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidGeneration);
    }
    let state = crate::stage5g_order_position::stage8b_p1d3_apply_truth_state(
        state,
        crate::Stage5gOrderPositionEvidence {
            total_sequence: seq_truth,
            request_id: feedback.request_id,
            broker_truth: feedback.truth.clone(),
            order_attribution: Some(feedback.expected_attribution.clone()),
        },
    )
    .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    feedback.phase = crate::stage8b_p1d2_market_feedback::Stage8bP1d2FeedbackPhase::TruthCommitted;
    feedback.feedback_projection_sha256 =
        crate::stage8b_p1d2_market_feedback::feedback_projection_sha256(&feedback);
    if !feedback.validate()
        || !crate::stage5g_order_position::stage8b_p1d4_truth_state_matches(
            &state,
            feedback.request_id,
            feedback.seq_ack,
            feedback.seq_truth,
            &feedback.truth,
        )
    {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
    }
    crate::stage6d_live_core::stage8b_p1d4_test_crash_frontier("GM11");
    let composition = composition.into_truth_committed()?;
    let fresh_runtime = runtime.stage5g_clean_reconstruction_candidate();
    let source = Stage8bP1d4RestartSource::new(
        runtime,
        state,
        p1d3,
        semantic,
        Some(feedback),
        composition.clone(),
    )?;
    let restart_package = export_stage5g_clean_restart(
        Stage5gCleanRestartSource::P1d4(Box::new(source)),
        export_input,
        commitment_key,
    )
    .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    let restored = restore_stage5g_clean_restart(&restart_package, commitment_key, fresh_runtime)
        .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidComposition)?;
    if restored.stage8b_p1d4_generated_market_composition() != Some(&composition) {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidComposition);
    }
    Ok(Stage8bP1d4TruthStageResult {
        restored,
        restart_package,
    })
}

pub fn stage8b_p1d4_immediate_redis_successor(
    value: &str,
) -> Result<String, Stage8bP1d4GeneratedMarketError> {
    let successor = immediate_successor(parse_redis_id(value)?)?;
    Ok(format!("{}-{}", successor.0, successor.1))
}

fn parse_redis_id(value: &str) -> Result<(u64, u64), Stage8bP1d4GeneratedMarketError> {
    let (milliseconds, sequence) = value
        .split_once('-')
        .ok_or(Stage8bP1d4GeneratedMarketError::InvalidIdentity)?;
    if milliseconds.is_empty()
        || sequence.is_empty()
        || milliseconds.contains('-')
        || sequence.contains('-')
        || (milliseconds.len() > 1 && milliseconds.starts_with('0'))
        || (sequence.len() > 1 && sequence.starts_with('0'))
        || !milliseconds.bytes().all(|byte| byte.is_ascii_digit())
        || !sequence.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidIdentity);
    }
    let milliseconds = milliseconds
        .parse::<u64>()
        .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidIdentity)?;
    let sequence = sequence
        .parse::<u64>()
        .map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidIdentity)?;
    Ok((milliseconds, sequence))
}

fn immediate_successor(value: (u64, u64)) -> Result<(u64, u64), Stage8bP1d4GeneratedMarketError> {
    if value.1 < u64::MAX {
        Ok((value.0, value.1 + 1))
    } else if value.0 < u64::MAX {
        Ok((value.0 + 1, 0))
    } else {
        Err(Stage8bP1d4GeneratedMarketError::InvalidIdentity)
    }
}

fn uuid_bytes(value: StrategyRequestId) -> [u8; 16] {
    *value.as_uuid().as_bytes()
}

fn lp_utf8(output: &mut Vec<u8>, value: &str) -> Result<(), Stage8bP1d4GeneratedMarketError> {
    if value.contains('\0') {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidEncoding);
    }
    let len =
        u32::try_from(value.len()).map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidEncoding)?;
    output.extend_from_slice(&len.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn raw_sha256(output: &mut Vec<u8>, value: &str) -> Result<(), Stage8bP1d4GeneratedMarketError> {
    if !is_sha256(value) {
        return Err(Stage8bP1d4GeneratedMarketError::InvalidEncoding);
    }
    for pair in value.as_bytes().chunks_exact(2) {
        output.push(
            (hex_nibble(pair[0]).ok_or(Stage8bP1d4GeneratedMarketError::InvalidEncoding)? << 4)
                | hex_nibble(pair[1]).ok_or(Stage8bP1d4GeneratedMarketError::InvalidEncoding)?,
        );
    }
    Ok(())
}

fn redis_id_bytes(
    output: &mut Vec<u8>,
    value: &str,
) -> Result<(), Stage8bP1d4GeneratedMarketError> {
    let (milliseconds, sequence) = parse_redis_id(value)?;
    output.extend_from_slice(&milliseconds.to_be_bytes());
    output.extend_from_slice(&sequence.to_be_bytes());
    Ok(())
}

fn framed(domain: &[u8], body: Vec<u8>) -> Result<Vec<u8>, Stage8bP1d4GeneratedMarketError> {
    let body_len =
        u64::try_from(body.len()).map_err(|_| Stage8bP1d4GeneratedMarketError::InvalidEncoding)?;
    let mut output = Vec::with_capacity(domain.len() + 8 + body.len());
    output.extend_from_slice(domain);
    output.extend_from_slice(&body_len.to_be_bytes());
    output.extend_from_slice(&body);
    Ok(output)
}

fn token(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct GoldenFixture {
        reservation: Stage8bP1d4CommandPublicationReservationV1,
        reservation_canonical_hex: String,
        binding: Stage8bP1d4CommandPublicationBindingV1,
        binding_canonical_hex: String,
    }

    fn hex(value: &[u8]) -> String {
        value.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn canonical_fixture_is_byte_exact() {
        let fixture: GoldenFixture = serde_json::from_str(include_str!(
            "../../../docs/stage-8/fixtures/stage8b-p1d4-command-publication-binding-v1.json"
        ))
        .unwrap();
        fixture.reservation.validate().unwrap();
        fixture
            .binding
            .validate_against(&fixture.reservation)
            .unwrap();
        assert_eq!(
            hex(&fixture.reservation.canonical_bytes().unwrap()),
            fixture.reservation_canonical_hex
        );
        assert_eq!(
            hex(&fixture
                .binding
                .canonical_bytes(&fixture.reservation)
                .unwrap()),
            fixture.binding_canonical_hex
        );
    }

    #[test]
    fn redis_successor_is_checked_and_canonical() {
        assert_eq!(
            stage8b_p1d4_immediate_redis_successor("1785999999999-3").unwrap(),
            "1785999999999-4"
        );
        assert_eq!(
            stage8b_p1d4_immediate_redis_successor(&format!("7-{}", u64::MAX)).unwrap(),
            "8-0"
        );
        assert!(
            stage8b_p1d4_immediate_redis_successor(&format!("{}-{}", u64::MAX, u64::MAX)).is_err()
        );
        for invalid in ["01-0", "0-01", "-1-0", "1", "1-2-3"] {
            assert!(stage8b_p1d4_immediate_redis_successor(invalid).is_err());
        }
    }

    #[test]
    fn seal_generation_above_json_safe_integer_remains_distinct() {
        let reservation = Stage8bP1d4CommandPublicationReservationV1::new(
            "source".to_string(),
            "source-group".to_string(),
            "10-1".to_string(),
            "11".repeat(32),
            StrategyRequestId::from(uuid::Uuid::from_u128(1)),
            "22".repeat(32),
            "33".repeat(32),
            "commands".to_string(),
            "command-group".to_string(),
            "20-8".to_string(),
            "20-9".to_string(),
            41,
        )
        .unwrap();
        let left = Stage8bP1d4CommandPublicationBindingV1::from_reservation(
            &reservation,
            9_007_199_254_740_993,
            "44".repeat(32),
        )
        .unwrap();
        let right = Stage8bP1d4CommandPublicationBindingV1::from_reservation(
            &reservation,
            9_007_199_254_740_994,
            "44".repeat(32),
        )
        .unwrap();
        assert_ne!(
            left.publication_binding_sha256(),
            right.publication_binding_sha256()
        );
        assert_ne!(
            left.canonical_bytes(&reservation).unwrap(),
            right.canonical_bytes(&reservation).unwrap()
        );
    }
}
