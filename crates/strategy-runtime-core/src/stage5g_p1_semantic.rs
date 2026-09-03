//! Stage 8B-P1-b semantic continuation over the accepted Stage 5G authority.
//!
//! The module has no Redis, provider, FINAM or broker-dispatch handle. It
//! consumes one restored Hybrid owner and returns either a restart-exportable
//! zero-intent state, a restart-exportable one-intent prepublication state, or
//! a terminal multi-intent diagnostic without continuation authority.

use broker_core::{BrokerCommand, HybridRuntimeAttribution, InstrumentId, StrategyRequestId};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    continue_stage5g_timer_with_bar, export_stage5g_clean_restart, settle_stage5g_bar_continuation,
    Stage5cAcceptedSemanticBar, Stage5gBarContinuationTransition, Stage5gCleanRestartError,
    Stage5gCleanRestartExportInput, Stage5gCleanRestartSource, Stage5gCleanRestartedCapability,
    Stage5gLifecycleCommitmentKey, Stage5gTimerGeneratedIntentEscrow,
    Stage5gTimerReadyPaperStrategy, Stage6DurableCommandSnapshotV1, Stage6DurableRequestIdentityV1,
};

pub const STAGE5G_P1_SEMANTIC_COMMIT_SCHEMA_VERSION: u16 = 1;
const P1_SEMANTIC_BATCH_DOMAIN: &str = "moex.stage8b.p1.semantic-batch.v1";

#[derive(Clone)]
pub struct Stage5gP1SemanticBindingInput {
    pub operational_identity_sha256: String,
    pub m10_redis_id: String,
    pub m10_semantic_id_sha256: String,
    pub m10_payload_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stage5gP1SemanticCommitProjectionV1 {
    pub(crate) schema_version: u16,
    pub(crate) identity_domain: String,
    pub(crate) operational_identity_sha256: String,
    pub(crate) m10_redis_id: String,
    pub(crate) m10_semantic_id_sha256: String,
    pub(crate) m10_payload_sha256: String,
    pub(crate) prior_stage5g_checkpoint_sha256: String,
    pub(crate) semantic_batch_id_sha256: String,
    pub(crate) intent_count: usize,
    pub(crate) request_id: Option<StrategyRequestId>,
    pub(crate) canonical_command: Option<BrokerCommand>,
    pub(crate) canonical_command_sha256: Option<String>,
    pub(crate) durable_request_identity: Option<Stage6DurableRequestIdentityV1>,
    pub(crate) durable_command_snapshot: Option<Stage6DurableCommandSnapshotV1>,
    pub(crate) expected_attribution: Option<HybridRuntimeAttribution>,
    pub(crate) source_intent: Option<crate::stage5c_paper_host::Stage5gSourceIntentProjection>,
}

impl Stage5gP1SemanticCommitProjectionV1 {
    pub(crate) fn validate(&self) -> bool {
        if self.schema_version != STAGE5G_P1_SEMANTIC_COMMIT_SCHEMA_VERSION
            || self.identity_domain != P1_SEMANTIC_BATCH_DOMAIN
            || !is_sha256(&self.operational_identity_sha256)
            || !is_sha256(&self.m10_semantic_id_sha256)
            || !is_sha256(&self.m10_payload_sha256)
            || !is_sha256(&self.prior_stage5g_checkpoint_sha256)
            || !is_sha256(&self.semantic_batch_id_sha256)
            || parse_exact_redis_id(&self.m10_redis_id).is_none()
            || self.semantic_batch_id_sha256
                != semantic_batch_id_sha256(
                    &self.operational_identity_sha256,
                    &self.m10_semantic_id_sha256,
                    &self.m10_payload_sha256,
                    &self.prior_stage5g_checkpoint_sha256,
                )
        {
            return false;
        }
        match (
            self.intent_count,
            self.request_id,
            self.canonical_command.as_ref(),
            self.canonical_command_sha256.as_deref(),
            self.durable_request_identity.as_ref(),
            self.durable_command_snapshot.as_ref(),
            self.expected_attribution.as_ref(),
            self.source_intent.as_ref(),
        ) {
            (0, None, None, None, None, None, None, None) => true,
            (
                1,
                Some(request_id),
                Some(command),
                Some(command_sha256),
                Some(identity),
                Some(snapshot),
                Some(attribution),
                Some(source),
            ) => {
                request_id == command_request_id(command)
                    && request_id == identity.strategy_request_id()
                    && request_id == source.request_id
                    && command_sha256
                        == sha256_hex(&serde_json::to_vec(command).unwrap_or_default())
                    && identity.attribution() == attribution
                    && source.expected_attribution.as_ref() == Some(attribution)
                    && snapshot.action() == identity.action()
                    && durable_command_matches(identity, snapshot, command, identity.instrument())
            }
            _ => false,
        }
    }
}

/// Opaque restart-exportable zero-intent source.
pub struct Stage5gP1ZeroIntentCommitted {
    pub(crate) ready: Stage5gTimerReadyPaperStrategy,
    pub(crate) projection: Stage5gP1SemanticCommitProjectionV1,
    export_input: Stage5gCleanRestartExportInput,
}

/// Opaque restart-exportable single-intent source. It grants no command
/// publication, provider invocation or dispatch authority.
pub struct Stage5gP1OneIntentPrepublication {
    pub(crate) escrow: Stage5gTimerGeneratedIntentEscrow,
    pub(crate) projection: Stage5gP1SemanticCommitProjectionV1,
    export_input: Stage5gCleanRestartExportInput,
}

pub struct Stage5gP1MultiIntentBlocked {
    semantic_batch_id_sha256: String,
    intent_count: usize,
    request_ids: Vec<StrategyRequestId>,
}

pub enum Stage5gP1SemanticTransition {
    ZeroIntent(Stage5gP1ZeroIntentCommitted),
    OneIntent(Stage5gP1OneIntentPrepublication),
    MultiIntentBlocked(Stage5gP1MultiIntentBlocked),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage5gP1SemanticFailure {
    #[error("P1 semantic source binding is invalid")]
    InvalidBinding,
    #[error("restored Stage 5G package is not P1 continuation-capable")]
    PriorAuthorityNotContinuationCapable,
    #[error("accepted Stage 5G semantic callback failed")]
    SemanticCallbackFailed,
    #[error("single generated intent is unsupported or cannot form a canonical command")]
    UnsupportedSingleIntent,
    #[error("next Stage 5G persistence identity overflowed")]
    PersistenceIdentityOverflow,
    #[error("Stage 5G restart export failed")]
    RestartExportFailed,
}

impl Stage5gP1ZeroIntentCommitted {
    pub fn semantic_batch_id_sha256(&self) -> &str {
        &self.projection.semantic_batch_id_sha256
    }

    pub fn m10_redis_id(&self) -> &str {
        &self.projection.m10_redis_id
    }

    pub fn intent_count(&self) -> usize {
        0
    }
}

impl Stage5gP1OneIntentPrepublication {
    pub fn semantic_batch_id_sha256(&self) -> &str {
        &self.projection.semantic_batch_id_sha256
    }

    pub fn m10_redis_id(&self) -> &str {
        &self.projection.m10_redis_id
    }

    pub fn request_id(&self) -> StrategyRequestId {
        self.projection
            .request_id
            .expect("validated one-intent projection has request id")
    }

    pub fn canonical_command(&self) -> &BrokerCommand {
        self.projection
            .canonical_command
            .as_ref()
            .expect("validated one-intent projection has command")
    }

    pub fn canonical_command_sha256(&self) -> &str {
        self.projection
            .canonical_command_sha256
            .as_deref()
            .expect("validated one-intent projection has command hash")
    }

    pub fn durable_request_identity(&self) -> &Stage6DurableRequestIdentityV1 {
        self.projection
            .durable_request_identity
            .as_ref()
            .expect("validated one-intent projection has durable identity")
    }

    pub fn durable_command_snapshot(&self) -> &Stage6DurableCommandSnapshotV1 {
        self.projection
            .durable_command_snapshot
            .as_ref()
            .expect("validated one-intent projection has durable command")
    }

    pub fn expected_attribution(&self) -> &HybridRuntimeAttribution {
        self.projection
            .expected_attribution
            .as_ref()
            .expect("validated one-intent projection has attribution")
    }
}

pub(crate) fn p1_projection_from_zero(
    value: &Stage5gP1ZeroIntentCommitted,
) -> &Stage5gP1SemanticCommitProjectionV1 {
    &value.projection
}

pub(crate) fn p1_projection_from_one(
    value: &Stage5gP1OneIntentPrepublication,
) -> &Stage5gP1SemanticCommitProjectionV1 {
    &value.projection
}

impl Stage5gP1MultiIntentBlocked {
    pub fn semantic_batch_id_sha256(&self) -> &str {
        &self.semantic_batch_id_sha256
    }

    pub fn intent_count(&self) -> usize {
        self.intent_count
    }

    pub fn request_ids(&self) -> &[StrategyRequestId] {
        &self.request_ids
    }

    pub fn command_publication_allowed(&self) -> bool {
        false
    }

    pub fn m10_xack_allowed(&self) -> bool {
        false
    }
}

pub fn continue_stage5g_p1_semantic(
    restored: Stage5gCleanRestartedCapability,
    accepted_bar: Stage5cAcceptedSemanticBar,
    binding: Stage5gP1SemanticBindingInput,
) -> Result<Stage5gP1SemanticTransition, Stage5gP1SemanticFailure> {
    if !is_sha256(&binding.operational_identity_sha256)
        || !is_sha256(&binding.m10_semantic_id_sha256)
        || !is_sha256(&binding.m10_payload_sha256)
        || parse_exact_redis_id(&binding.m10_redis_id).is_none()
    {
        return Err(Stage5gP1SemanticFailure::InvalidBinding);
    }
    let prior_checkpoint_sha256 = sha256_hex(
        &serde_json::to_vec(restored.checkpoint())
            .map_err(|_| Stage5gP1SemanticFailure::InvalidBinding)?,
    );
    let semantic_batch_id_sha256 = semantic_batch_id_sha256(
        &binding.operational_identity_sha256,
        &binding.m10_semantic_id_sha256,
        &binding.m10_payload_sha256,
        &prior_checkpoint_sha256,
    );
    let export_input = restored
        .stage8b_p1_next_export_input(
            &semantic_batch_id_sha256,
            parse_exact_redis_id(&binding.m10_redis_id).unwrap(),
        )
        .map_err(|_| Stage5gP1SemanticFailure::PersistenceIdentityOverflow)?;
    let ready = restored
        .into_stage8b_p1_timer_ready()
        .map_err(|_| Stage5gP1SemanticFailure::PriorAuthorityNotContinuationCapable)?;
    let continued = continue_stage5g_timer_with_bar(ready, accepted_bar)
        .map_err(|_| Stage5gP1SemanticFailure::SemanticCallbackFailed)?;
    match settle_stage5g_bar_continuation(continued) {
        Stage5gBarContinuationTransition::Ready(ready) => {
            let projection = Stage5gP1SemanticCommitProjectionV1 {
                schema_version: STAGE5G_P1_SEMANTIC_COMMIT_SCHEMA_VERSION,
                identity_domain: P1_SEMANTIC_BATCH_DOMAIN.to_string(),
                operational_identity_sha256: binding.operational_identity_sha256,
                m10_redis_id: binding.m10_redis_id,
                m10_semantic_id_sha256: binding.m10_semantic_id_sha256,
                m10_payload_sha256: binding.m10_payload_sha256,
                prior_stage5g_checkpoint_sha256: prior_checkpoint_sha256,
                semantic_batch_id_sha256,
                intent_count: 0,
                request_id: None,
                canonical_command: None,
                canonical_command_sha256: None,
                durable_request_identity: None,
                durable_command_snapshot: None,
                expected_attribution: None,
                source_intent: None,
            };
            if !projection.validate() {
                return Err(Stage5gP1SemanticFailure::InvalidBinding);
            }
            Ok(Stage5gP1SemanticTransition::ZeroIntent(
                Stage5gP1ZeroIntentCommitted {
                    ready,
                    projection,
                    export_input,
                },
            ))
        }
        Stage5gBarContinuationTransition::GeneratedIntent(escrow) => {
            let intent_count = escrow.intent_count();
            if let Some(blocked) =
                p1_multi_intent_boundary(&semantic_batch_id_sha256, escrow.request_ids())
            {
                return Ok(Stage5gP1SemanticTransition::MultiIntentBlocked(blocked));
            }
            if intent_count != 1 {
                return Err(Stage5gP1SemanticFailure::SemanticCallbackFailed);
            }
            let material = escrow
                .stage8b_p1_command_material()
                .map_err(|_| Stage5gP1SemanticFailure::UnsupportedSingleIntent)?;
            let request_id = command_request_id(&material.command);
            let (durable_request_identity, durable_command_snapshot) = durable_command_material(
                &material.command,
                material.instrument.clone(),
                material.expected_attribution.clone(),
            )
            .ok_or(Stage5gP1SemanticFailure::UnsupportedSingleIntent)?;
            if request_id != material.source.request_id {
                return Err(Stage5gP1SemanticFailure::UnsupportedSingleIntent);
            }
            let canonical_command_sha256 = sha256_hex(
                &serde_json::to_vec(&material.command)
                    .map_err(|_| Stage5gP1SemanticFailure::UnsupportedSingleIntent)?,
            );
            let projection = Stage5gP1SemanticCommitProjectionV1 {
                schema_version: STAGE5G_P1_SEMANTIC_COMMIT_SCHEMA_VERSION,
                identity_domain: P1_SEMANTIC_BATCH_DOMAIN.to_string(),
                operational_identity_sha256: binding.operational_identity_sha256,
                m10_redis_id: binding.m10_redis_id,
                m10_semantic_id_sha256: binding.m10_semantic_id_sha256,
                m10_payload_sha256: binding.m10_payload_sha256,
                prior_stage5g_checkpoint_sha256: prior_checkpoint_sha256,
                semantic_batch_id_sha256,
                intent_count: 1,
                request_id: Some(request_id),
                canonical_command: Some(material.command),
                canonical_command_sha256: Some(canonical_command_sha256),
                durable_request_identity: Some(durable_request_identity),
                durable_command_snapshot: Some(durable_command_snapshot),
                expected_attribution: Some(material.expected_attribution),
                source_intent: Some(material.source),
            };
            if !projection.validate() {
                return Err(Stage5gP1SemanticFailure::UnsupportedSingleIntent);
            }
            Ok(Stage5gP1SemanticTransition::OneIntent(
                Stage5gP1OneIntentPrepublication {
                    escrow,
                    projection,
                    export_input,
                },
            ))
        }
    }
}

fn p1_multi_intent_boundary(
    semantic_batch_id_sha256: &str,
    request_ids: &[StrategyRequestId],
) -> Option<Stage5gP1MultiIntentBlocked> {
    (request_ids.len() > 1).then(|| Stage5gP1MultiIntentBlocked {
        semantic_batch_id_sha256: semantic_batch_id_sha256.to_string(),
        intent_count: request_ids.len(),
        request_ids: request_ids.to_vec(),
    })
}

pub fn export_stage5g_p1_zero_intent(
    source: Stage5gP1ZeroIntentCommitted,
    key: &Stage5gLifecycleCommitmentKey,
) -> Result<Vec<u8>, Stage5gP1SemanticFailure> {
    let Stage5gP1ZeroIntentCommitted {
        ready,
        projection,
        export_input,
    } = source;
    export_stage5g_clean_restart(
        Stage5gCleanRestartSource::P1SemanticReady(Stage5gP1ZeroIntentCommitted {
            ready,
            projection,
            export_input: export_input.clone(),
        }),
        export_input,
        key,
    )
    .map_err(|_| Stage5gP1SemanticFailure::RestartExportFailed)
}

pub fn export_stage5g_p1_one_intent(
    source: Stage5gP1OneIntentPrepublication,
    key: &Stage5gLifecycleCommitmentKey,
) -> Result<Vec<u8>, Stage5gP1SemanticFailure> {
    let Stage5gP1OneIntentPrepublication {
        escrow,
        projection,
        export_input,
    } = source;
    export_stage5g_clean_restart(
        Stage5gCleanRestartSource::P1SemanticPrepublication(Stage5gP1OneIntentPrepublication {
            escrow,
            projection,
            export_input: export_input.clone(),
        }),
        export_input,
        key,
    )
    .map_err(|_| Stage5gP1SemanticFailure::RestartExportFailed)
}

pub(crate) fn p1_zero_source_runtime(
    value: &Stage5gP1ZeroIntentCommitted,
) -> &crate::HybridIntradayRuntimeStrategy {
    value.ready.stage5g_runtime_strategy()
}

pub(crate) fn p1_zero_source_binding(
    value: &Stage5gP1ZeroIntentCommitted,
) -> (
    &str,
    &broker_core::BrokerAccountId,
    &broker_core::InstrumentId,
) {
    value.ready.stage5g_restart_binding()
}

pub(crate) fn p1_one_source_runtime(
    value: &Stage5gP1OneIntentPrepublication,
) -> &crate::HybridIntradayRuntimeStrategy {
    value.escrow.stage8b_p1_runtime_strategy()
}

pub(crate) fn p1_one_source_binding(
    value: &Stage5gP1OneIntentPrepublication,
) -> (
    &str,
    &broker_core::BrokerAccountId,
    &broker_core::InstrumentId,
) {
    value.escrow.stage8b_p1_restart_binding()
}

pub(crate) fn p1_prepublication_restart_slot(
    projection: &Stage5gP1SemanticCommitProjectionV1,
) -> Option<crate::stage5g_order_position::Stage5gFreshTruthRestartSlotProjection> {
    use crate::stage5g_mock_ack::{Stage5gMockIntentAction, Stage5gMockPlaceKind};
    use crate::stage5g_order_position::{
        stage5g_attribution_fingerprint_sha256, stage5g_integral_lot_decimal,
        Stage5gCancelTargetOrderAuthority,
    };

    if !projection.validate() || projection.intent_count != 1 {
        return None;
    }
    let command = projection.canonical_command.as_ref()?;
    let identity = projection.durable_request_identity.as_ref()?;
    let source = projection.source_intent.as_ref()?;
    let attribution = projection.expected_attribution.as_ref()?;
    let pre_position_qty = stage5g_integral_lot_decimal(source.pre_position_qty)?;
    let target_qty = source.target_qty.and_then(stage5g_integral_lot_decimal);
    let (source_action, side, command_qty, target_broker_order_id, target_client_order_id) =
        match command {
            BrokerCommand::PlaceOrder(place) => (
                Stage5gMockIntentAction::Place {
                    place_kind: match place.order_type {
                        broker_core::OrderType::Market => Stage5gMockPlaceKind::Market,
                        broker_core::OrderType::Limit => Stage5gMockPlaceKind::Limit,
                        _ => return None,
                    },
                },
                Some(place.side),
                Some(place.qty),
                None,
                None,
            ),
            BrokerCommand::CancelOrder(cancel) => (
                Stage5gMockIntentAction::Cancel {
                    target_order_id: cancel.order_id.clone(),
                },
                None,
                None,
                Some(cancel.order_id.clone()),
                cancel.client_order_id.clone(),
            ),
        };
    if command_qty != target_qty {
        return None;
    }
    let cancel_target_order_authority =
        target_broker_order_id
            .as_ref()
            .map(|target| Stage5gCancelTargetOrderAuthority {
                target_broker_order_id: target.clone(),
                target_order_client_order_id: target_client_order_id.clone(),
                immutable_order_commitment_sha256: None,
            });
    Some(
        crate::stage5g_order_position::Stage5gFreshTruthRestartSlotProjection {
            command_request_id: identity.strategy_request_id().to_string(),
            command_client_order_id: identity.durable_client_order_id().clone(),
            target_broker_order_id,
            target_order_client_order_id: target_client_order_id,
            cancel_target_order_authority,
            intent_class: source.intent_class.into(),
            source_action,
            side,
            target_qty,
            pre_position_qty,
            source_numeric_authority_is_integral: true,
            expected_attribution_fingerprint_sha256: Some(stage5g_attribution_fingerprint_sha256(
                attribution,
            )),
            latest_order: None,
            trades: Vec::new(),
            position: None,
            terminal: false,
        },
    )
}

fn semantic_batch_id_sha256(
    operational_identity_sha256: &str,
    m10_semantic_id_sha256: &str,
    m10_payload_sha256: &str,
    prior_stage5g_checkpoint_sha256: &str,
) -> String {
    #[derive(Serialize)]
    struct BatchIdentity<'a> {
        operational_identity_sha256: &'a str,
        m10_semantic_id_sha256: &'a str,
        m10_payload_sha256: &'a str,
        prior_stage5g_checkpoint_sha256: &'a str,
    }
    let bytes = serde_json::to_vec(&BatchIdentity {
        operational_identity_sha256,
        m10_semantic_id_sha256,
        m10_payload_sha256,
        prior_stage5g_checkpoint_sha256,
    })
    .expect("typed P1 semantic batch remains serializable");
    let mut hasher = Sha256::new();
    hasher.update(P1_SEMANTIC_BATCH_DOMAIN.as_bytes());
    hasher.update(b"\0");
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn durable_command_material(
    command: &BrokerCommand,
    instrument: InstrumentId,
    attribution: HybridRuntimeAttribution,
) -> Option<(
    Stage6DurableRequestIdentityV1,
    Stage6DurableCommandSnapshotV1,
)> {
    match command {
        BrokerCommand::PlaceOrder(place) => {
            let identity = Stage6DurableRequestIdentityV1::from_place(place, attribution).ok()?;
            let snapshot = Stage6DurableCommandSnapshotV1::from_place(&identity, place).ok()?;
            Some((identity, snapshot))
        }
        BrokerCommand::CancelOrder(cancel) => {
            let identity =
                Stage6DurableRequestIdentityV1::from_cancel(cancel, instrument, attribution)
                    .ok()?;
            let snapshot = Stage6DurableCommandSnapshotV1::from_cancel(&identity, cancel).ok()?;
            Some((identity, snapshot))
        }
    }
}

fn durable_command_matches(
    identity: &Stage6DurableRequestIdentityV1,
    snapshot: &Stage6DurableCommandSnapshotV1,
    command: &BrokerCommand,
    instrument: &InstrumentId,
) -> bool {
    durable_command_material(command, instrument.clone(), identity.attribution().clone())
        .is_some_and(|(expected_identity, expected_snapshot)| {
            &expected_identity == identity && &expected_snapshot == snapshot
        })
}

fn command_request_id(command: &BrokerCommand) -> StrategyRequestId {
    match command {
        BrokerCommand::PlaceOrder(command) => command.request_id,
        BrokerCommand::CancelOrder(command) => command.request_id,
    }
}

fn parse_exact_redis_id(value: &str) -> Option<i64> {
    let millis = value.strip_suffix("-0")?.parse::<i64>().ok()?;
    if millis <= 0 || Utc.timestamp_millis_opt(millis).single().is_none() {
        return None;
    }
    Some(millis)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl From<Stage5gCleanRestartError> for Stage5gP1SemanticFailure {
    fn from(_: Stage5gCleanRestartError) -> Self {
        Self::RestartExportFailed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        Exchange, HybridRuntimeBarEvent, HybridRuntimeBarOrigin, InstrumentId, Market,
        Stage3StrategyBarProvenance,
    };

    fn restored_source() -> Stage5gCleanRestartedCapability {
        let (ready, export_input, key, fresh_runtime) =
            crate::stage5g_timer::stage8b_p1_test_first_boot_material();
        let bytes = crate::export_stage5g_clean_restart(
            crate::Stage5gCleanRestartSource::P1BootstrapReady(ready),
            export_input,
            &key,
        )
        .unwrap();
        crate::restore_stage5g_clean_restart(&bytes, &key, fresh_runtime).unwrap()
    }

    fn accepted_bar(close: f64) -> Stage5cAcceptedSemanticBar {
        let close_time_utc = Utc
            .with_ymd_and_hms(2026, 8, 3, 12, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        crate::accept_stage5c_semantic_bar(crate::Stage5cSemanticBarInput {
            bar: HybridRuntimeBarEvent {
                instrument: InstrumentId {
                    symbol: "IMOEXF".to_string(),
                    venue_symbol: Some("IMOEXF@RTSX".to_string()),
                    exchange: Exchange::Moex,
                    market: Market::Futures,
                },
                close_time_utc,
                open: close,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 10_000.0,
                origin: HybridRuntimeBarOrigin::Live,
                is_final: true,
                timeframe_sec: 600,
            },
            provenance: Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
            tick_size: 0.5,
        })
        .unwrap()
    }

    fn binding() -> Stage5gP1SemanticBindingInput {
        Stage5gP1SemanticBindingInput {
            operational_identity_sha256: "11".repeat(32),
            m10_redis_id: "1785759000000-0".to_string(),
            m10_semantic_id_sha256: "22".repeat(32),
            m10_payload_sha256: "33".repeat(32),
        }
    }

    #[test]
    fn p1_zero_intent_semantic_transition_is_restart_exportable() {
        let transition =
            continue_stage5g_p1_semantic(restored_source(), accepted_bar(2_600.0), binding())
                .unwrap();
        let Stage5gP1SemanticTransition::ZeroIntent(zero) = transition else {
            panic!("unchanged M10 must remain zero-intent");
        };
        assert_eq!(zero.intent_count(), 0);
        assert_eq!(zero.m10_redis_id(), "1785759000000-0");
    }

    #[test]
    fn p1_breakout_m10_produces_one_deterministic_command() {
        let transition =
            continue_stage5g_p1_semantic(restored_source(), accepted_bar(2_650.0), binding())
                .unwrap();
        let Stage5gP1SemanticTransition::OneIntent(one) = transition else {
            panic!("breakout M10 must produce exactly one intent");
        };
        assert_eq!(one.m10_redis_id(), "1785759000000-0");
        assert_eq!(
            one.request_id(),
            one.durable_request_identity().strategy_request_id()
        );
        assert_eq!(one.canonical_command_sha256().len(), 64);
        assert!(one.expected_attribution().belongs_to("hybrid_imoexf"));
    }

    #[test]
    fn p1_multi_intent_boundary_returns_only_noncontinuable_diagnostic() {
        let request_ids = [
            StrategyRequestId::from(uuid::Uuid::from_u128(1)),
            StrategyRequestId::from(uuid::Uuid::from_u128(2)),
        ];
        let blocked = p1_multi_intent_boundary(&"aa".repeat(32), &request_ids).unwrap();
        assert_eq!(blocked.intent_count(), 2);
        assert_eq!(blocked.request_ids(), request_ids);
        assert!(!blocked.command_publication_allowed());
        assert!(!blocked.m10_xack_allowed());
        assert!(p1_multi_intent_boundary(&"aa".repeat(32), &request_ids[..1]).is_none());
    }
}
