//! Stage 8B-P1-d1 deterministic Market paper-provider core.
//!
//! This module deliberately contains no Redis, FINAM, wall-clock acquisition,
//! ACK, broker projection or runtime-live operation.  The production wait
//! state can only be minted inside this crate from the retained Stage 5E
//! schedule projection.  A provider outcome additionally requires the linear
//! Stage 6 durable dispatch receipt, so observing a price bar alone never
//! grants an effect capability.
//!
//! The wait and eligibility states intentionally expose no provider call:
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage8bP1d1AwaitingExecutionBar;
//! let waiting: Stage8bP1d1AwaitingExecutionBar = unreachable!();
//! let _ = waiting.execute();
//! ```
//!
//! ```compile_fail
//! use strategy_runtime_core::Stage8bP1d1ExecutionEligible;
//! let eligible: Stage8bP1d1ExecutionEligible = unreachable!();
//! let _ = eligible.execute();
//! ```

use broker_core::{
    BrokerCommand, BrokerOrderId, BrokerTradeId, InstrumentId, OrderType, StrategyRequestId,
    TimeInForce,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};

use crate::stage5e_no_io_lifecycle::schedule_window_evidence::{
    classify_stage8b_p1d1_execution_bar, Stage5eScheduleProjectionBridgeInput,
    Stage8bP1d1ScheduleBridgeBlockReason, Stage8bP1d1ScheduleProjectionApproved,
};
use crate::{Stage6DurableActionKind, Stage6dPaperDispatchReceipt, Stage6dPaperOutcome};

pub const STAGE8B_P1D1_EXECUTION_POLICY_DOMAIN: &str = "moex.stage8b.p1d.execution-policy.v1";
pub const STAGE8B_P1D1_ORDER_ID_DOMAIN: &str = "moex.stage8b.p1d.order-id.v1";
pub const STAGE8B_P1D1_TRADE_ID_DOMAIN: &str = "moex.stage8b.p1d.trade-id.v1";

const M10_MILLIS: i64 = 600_000;

/// Strict canonical-bar facts supplied by the already validated P1 M10
/// decoder.  This DTO is evidence input only; it grants no eligibility or
/// provider authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1d1CanonicalExecutionBar {
    pub operational_identity_sha256: String,
    pub instrument: InstrumentId,
    pub semantic_id_sha256: String,
    pub payload_sha256: String,
    pub open_ts_utc_ms: i64,
    pub close_ts_utc_ms: i64,
    pub open: Decimal,
    pub timeframe_sec: u32,
    pub is_final: bool,
    pub is_live: bool,
}

/// Exact command/predecessor binding carried beside one read-only execution
/// bar observation.  Callers may construct evidence, but cannot turn it into
/// eligibility without the private Stage 5E schedule projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1d1ExecutionBarObservation {
    pub strategy_request_id: StrategyRequestId,
    pub canonical_command_sha256: String,
    pub predecessor_semantic_id_sha256: String,
    pub bar: Stage8bP1d1CanonicalExecutionBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bP1d1ExecutionEligibilityBlockReason {
    MultipleCandidates,
    WrongRequest,
    WrongCommand,
    WrongPredecessor,
    WrongOperationalIdentity,
    WrongInstrument,
    NonCanonicalIdentity,
    HistoryOrWarmup,
    NonFinal,
    InvalidTimeframe,
    InvalidChronology,
    SameBar,
    CrossTradingDay,
    ExecutionBarGap,
    ScheduleNotYetObserved,
    ScheduleExpired,
    CandidateObservedInFuture,
    ScheduleRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1d1ProviderError {
    #[error("P1-d1 accepts Market PLACE commands only")]
    UnsupportedCommand,
    #[error("P1-d1 Market v1 accepts Day time-in-force only")]
    UnsupportedTimeInForce,
    #[error("P1-d1 policy forbids command TTL")]
    TtlForbidden,
    #[error("canonical command binding is invalid")]
    CommandBindingMismatch,
    #[error("durable dispatch receipt does not match eligibility")]
    DispatchBindingMismatch,
}

/// Retryable no-effect owner.  Its constructor is crate-private and consumes
/// the opaque Stage 5E schedule projection.  It is neither Clone nor
/// serializable.
pub struct Stage8bP1d1AwaitingExecutionBar {
    schedule_projection: Stage5eScheduleProjectionBridgeInput,
    command: BrokerCommand,
    operational_identity_sha256: String,
    canonical_command_sha256: String,
    predecessor_semantic_id_sha256: String,
    predecessor_close_ts_utc_ms: i64,
}

/// Blocked observation keeps the linear wait owner but grants no dispatch or
/// provider method.  A refreshed exact observation may be retried explicitly.
pub struct Stage8bP1d1ExecutionEligibilityBlocked {
    reason: Stage8bP1d1ExecutionEligibilityBlockReason,
    awaiting: Stage8bP1d1AwaitingExecutionBar,
}

impl Stage8bP1d1ExecutionEligibilityBlocked {
    pub fn reason(&self) -> Stage8bP1d1ExecutionEligibilityBlockReason {
        self.reason
    }

    pub fn into_retry(self) -> Stage8bP1d1AwaitingExecutionBar {
        self.awaiting
    }
}

/// Observation has exactly three outcomes.  `NoInput` and `Blocked` retain
/// the wait owner and therefore have no durable-dispatch operation.
pub enum Stage8bP1d1ExecutionObservation {
    NoInput(Box<Stage8bP1d1AwaitingExecutionBar>),
    Eligible(Box<Stage8bP1d1ExecutionEligible>),
    Blocked(Box<Stage8bP1d1ExecutionEligibilityBlocked>),
}

/// Opaque exact next-bar capability.  It exposes no constructor and is
/// consumed when it is bound to the Stage 6 dispatch receipt.
pub struct Stage8bP1d1ExecutionEligible {
    command: BrokerCommand,
    operational_identity_sha256: String,
    canonical_command_sha256: String,
    predecessor_semantic_id_sha256: String,
    execution_bar: Stage8bP1d1CanonicalExecutionBar,
    schedule_approval: Stage8bP1d1ScheduleProjectionApproved,
}

/// Post-dispatch provider capability.  The receipt remains owned here until
/// the deterministic outcome is derived.
pub struct Stage8bP1d1MarketDispatchReady {
    eligibility: Stage8bP1d1ExecutionEligible,
    dispatch_receipt: Stage6dPaperDispatchReceipt,
}

/// Linear result retained for P1-d2 composition.  It contains the exact
/// Stage 6 outcome plus deterministic fill evidence, but cannot ACK, XACK or
/// mutate Hybrid state in this stage.
pub struct Stage8bP1d1MarketOutcomeBundle {
    dispatch_receipt: Stage6dPaperDispatchReceipt,
    stage6_outcome: Stage6dPaperOutcome,
    strategy_request_id: StrategyRequestId,
    canonical_command_sha256: String,
    execution_bar_semantic_id_sha256: String,
    execution_bar_payload_sha256: String,
    fill_price: Decimal,
    fill_qty: Decimal,
    fill_source_ts_utc_ms: i64,
    broker_order_id: BrokerOrderId,
    broker_trade_id: BrokerTradeId,
}

impl Stage8bP1d1MarketOutcomeBundle {
    pub fn strategy_request_id(&self) -> StrategyRequestId {
        self.strategy_request_id
    }

    pub fn canonical_command_sha256(&self) -> &str {
        &self.canonical_command_sha256
    }

    pub fn execution_bar_semantic_id_sha256(&self) -> &str {
        &self.execution_bar_semantic_id_sha256
    }

    pub fn execution_bar_payload_sha256(&self) -> &str {
        &self.execution_bar_payload_sha256
    }

    pub fn fill_price(&self) -> Decimal {
        self.fill_price
    }

    pub fn fill_qty(&self) -> Decimal {
        self.fill_qty
    }

    pub fn fill_source_ts_utc_ms(&self) -> i64 {
        self.fill_source_ts_utc_ms
    }

    pub fn broker_order_id(&self) -> &BrokerOrderId {
        &self.broker_order_id
    }

    pub fn broker_trade_id(&self) -> &BrokerTradeId {
        &self.broker_trade_id
    }

    pub fn feedback_application_allowed(&self) -> bool {
        false
    }

    pub fn ack_allowed(&self) -> bool {
        false
    }

    pub fn source_m10_xack_allowed(&self) -> bool {
        false
    }

    #[allow(
        dead_code,
        reason = "opened only by the separately reviewed P1-d2 composition"
    )]
    pub(crate) fn into_stage6_parts(self) -> (Stage6dPaperDispatchReceipt, Stage6dPaperOutcome) {
        (self.dispatch_receipt, self.stage6_outcome)
    }
}

impl Stage8bP1d1AwaitingExecutionBar {
    pub fn observe_candidates(
        self,
        candidates: Vec<Stage8bP1d1ExecutionBarObservation>,
        observed_at: DateTime<Utc>,
    ) -> Stage8bP1d1ExecutionObservation {
        if candidates.is_empty() {
            return Stage8bP1d1ExecutionObservation::NoInput(Box::new(self));
        }
        if candidates.len() != 1 {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::MultipleCandidates);
        }
        let candidate = candidates.into_iter().next().expect("length checked");
        if candidate.strategy_request_id != command_request_id(&self.command) {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::WrongRequest);
        }
        if candidate.canonical_command_sha256 != self.canonical_command_sha256 {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::WrongCommand);
        }
        if candidate.predecessor_semantic_id_sha256 != self.predecessor_semantic_id_sha256 {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::WrongPredecessor);
        }
        if candidate.bar.operational_identity_sha256 != self.operational_identity_sha256 {
            return self
                .block(Stage8bP1d1ExecutionEligibilityBlockReason::WrongOperationalIdentity);
        }
        let place = match &self.command {
            BrokerCommand::PlaceOrder(place) => place,
            BrokerCommand::CancelOrder(_) => {
                return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::WrongCommand)
            }
        };
        if candidate.bar.instrument != place.instrument {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::WrongInstrument);
        }
        if !is_sha256(&candidate.bar.semantic_id_sha256)
            || !is_sha256(&candidate.bar.payload_sha256)
            || !is_sha256(&candidate.bar.operational_identity_sha256)
        {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::NonCanonicalIdentity);
        }
        if !candidate.bar.is_live {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::HistoryOrWarmup);
        }
        if !candidate.bar.is_final {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::NonFinal);
        }
        if candidate.bar.timeframe_sec != 600 {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::InvalidTimeframe);
        }
        if candidate.bar.open_ts_utc_ms <= 0
            || candidate.bar.close_ts_utc_ms <= 0
            || candidate.bar.close_ts_utc_ms - candidate.bar.open_ts_utc_ms != M10_MILLIS
            || candidate.bar.close_ts_utc_ms.rem_euclid(M10_MILLIS) != 0
            || candidate.bar.open <= Decimal::ZERO
        {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::InvalidChronology);
        }
        if candidate.bar.close_ts_utc_ms == self.predecessor_close_ts_utc_ms {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::SameBar);
        }
        if candidate.bar.close_ts_utc_ms < self.predecessor_close_ts_utc_ms {
            return self.block(Stage8bP1d1ExecutionEligibilityBlockReason::InvalidChronology);
        }

        let Stage8bP1d1AwaitingExecutionBar {
            schedule_projection,
            command,
            operational_identity_sha256,
            canonical_command_sha256,
            predecessor_semantic_id_sha256,
            predecessor_close_ts_utc_ms,
        } = self;
        match classify_stage8b_p1d1_execution_bar(
            schedule_projection,
            &candidate.bar.instrument,
            predecessor_close_ts_utc_ms.div_euclid(1_000),
            candidate.bar.close_ts_utc_ms.div_euclid(1_000),
            observed_at,
        ) {
            Ok(schedule_approval) => {
                Stage8bP1d1ExecutionObservation::Eligible(Box::new(Stage8bP1d1ExecutionEligible {
                    command,
                    operational_identity_sha256,
                    canonical_command_sha256,
                    predecessor_semantic_id_sha256,
                    execution_bar: candidate.bar,
                    schedule_approval,
                }))
            }
            Err(blocked) => {
                let (reason, schedule_projection) = (*blocked).into_parts();
                let reason = map_schedule_block(reason);
                Stage8bP1d1ExecutionObservation::Blocked(Box::new(
                    Stage8bP1d1ExecutionEligibilityBlocked {
                        reason,
                        awaiting: Stage8bP1d1AwaitingExecutionBar {
                            schedule_projection,
                            command,
                            operational_identity_sha256,
                            canonical_command_sha256,
                            predecessor_semantic_id_sha256,
                            predecessor_close_ts_utc_ms,
                        },
                    },
                ))
            }
        }
    }

    fn block(
        self,
        reason: Stage8bP1d1ExecutionEligibilityBlockReason,
    ) -> Stage8bP1d1ExecutionObservation {
        Stage8bP1d1ExecutionObservation::Blocked(Box::new(Stage8bP1d1ExecutionEligibilityBlocked {
            reason,
            awaiting: self,
        }))
    }
}

/// The only P1-d1 entry seam.  It remains crate-private until a later stage
/// wires the accepted P1-c owner to a source-produced schedule projection.
#[allow(
    dead_code,
    reason = "P1-d1 core is sealed until separately reviewed P1-d2 composition"
)]
pub(crate) fn begin_stage8b_p1d1_market_wait(
    schedule_projection: Stage5eScheduleProjectionBridgeInput,
    command: BrokerCommand,
    operational_identity_sha256: String,
    canonical_command_sha256: String,
    predecessor_semantic_id_sha256: String,
    predecessor_close_ts_utc_ms: i64,
) -> Result<Stage8bP1d1AwaitingExecutionBar, Stage8bP1d1ProviderError> {
    let place = match &command {
        BrokerCommand::PlaceOrder(place) if place.order_type == OrderType::Market => place,
        _ => return Err(Stage8bP1d1ProviderError::UnsupportedCommand),
    };
    if place.time_in_force != TimeInForce::Day {
        return Err(Stage8bP1d1ProviderError::UnsupportedTimeInForce);
    }
    if place.ttl_ms.is_some() {
        return Err(Stage8bP1d1ProviderError::TtlForbidden);
    }
    if place.limit_price.is_some()
        || place.qty <= Decimal::ZERO
        || !is_sha256(&operational_identity_sha256)
        || !is_sha256(&canonical_command_sha256)
        || !is_sha256(&predecessor_semantic_id_sha256)
        || predecessor_close_ts_utc_ms <= 0
        || predecessor_close_ts_utc_ms.rem_euclid(M10_MILLIS) != 0
    {
        return Err(Stage8bP1d1ProviderError::CommandBindingMismatch);
    }
    let bytes = serde_json::to_vec(&command)
        .map_err(|_| Stage8bP1d1ProviderError::CommandBindingMismatch)?;
    if sha256_hex(&bytes) != canonical_command_sha256 {
        return Err(Stage8bP1d1ProviderError::CommandBindingMismatch);
    }
    Ok(Stage8bP1d1AwaitingExecutionBar {
        schedule_projection,
        command,
        operational_identity_sha256,
        canonical_command_sha256,
        predecessor_semantic_id_sha256,
        predecessor_close_ts_utc_ms,
    })
}

/// Consumes both exact eligibility and the fsync-backed Stage 6 dispatch
/// receipt.  No overload accepts raw IDs or a boolean substitute.
pub fn bind_stage8b_p1d1_market_dispatch(
    eligibility: Stage8bP1d1ExecutionEligible,
    dispatch_receipt: Stage6dPaperDispatchReceipt,
) -> Result<Stage8bP1d1MarketDispatchReady, Stage8bP1d1ProviderError> {
    let place = match &eligibility.command {
        BrokerCommand::PlaceOrder(place) => place,
        BrokerCommand::CancelOrder(_) => {
            return Err(Stage8bP1d1ProviderError::DispatchBindingMismatch)
        }
    };
    let identity = dispatch_receipt.stage8b_p1d1_identity();
    if identity.action() != Stage6DurableActionKind::Place
        || identity.strategy_request_id() != place.request_id
        || identity.account_id() != &place.account_id
        || identity.instrument() != &place.instrument
    {
        return Err(Stage8bP1d1ProviderError::DispatchBindingMismatch);
    }
    Ok(Stage8bP1d1MarketDispatchReady {
        eligibility,
        dispatch_receipt,
    })
}

impl Stage8bP1d1MarketDispatchReady {
    /// Pure deterministic provider call.  IDs, price and source timestamp are
    /// all functions of already durable/validated inputs.
    pub fn execute(self) -> Stage8bP1d1MarketOutcomeBundle {
        let Stage8bP1d1MarketDispatchReady {
            eligibility,
            dispatch_receipt,
        } = self;
        let place = match &eligibility.command {
            BrokerCommand::PlaceOrder(place) => place,
            BrokerCommand::CancelOrder(_) => unreachable!("eligibility is Market PLACE only"),
        };
        let broker_order_id = derive_order_id(
            &eligibility.operational_identity_sha256,
            place.request_id,
            &eligibility.canonical_command_sha256,
        );
        let broker_trade_id = derive_trade_id(
            &broker_order_id,
            &eligibility.execution_bar.semantic_id_sha256,
        );
        let _opaque_schedule_binding = (
            eligibility.schedule_approval.identity_fingerprint(),
            eligibility.schedule_approval.expires_at(),
            &eligibility.predecessor_semantic_id_sha256,
        );
        let stage6_outcome = Stage6dPaperOutcome::MarketFilled {
            broker_order_id: broker_order_id.clone(),
            broker_trade_id: broker_trade_id.clone(),
        };
        Stage8bP1d1MarketOutcomeBundle {
            dispatch_receipt,
            stage6_outcome,
            strategy_request_id: place.request_id,
            canonical_command_sha256: eligibility.canonical_command_sha256,
            execution_bar_semantic_id_sha256: eligibility.execution_bar.semantic_id_sha256,
            execution_bar_payload_sha256: eligibility.execution_bar.payload_sha256,
            fill_price: eligibility.execution_bar.open,
            fill_qty: place.qty,
            fill_source_ts_utc_ms: eligibility.execution_bar.open_ts_utc_ms,
            broker_order_id,
            broker_trade_id,
        }
    }
}

fn command_request_id(command: &BrokerCommand) -> StrategyRequestId {
    match command {
        BrokerCommand::PlaceOrder(place) => place.request_id,
        BrokerCommand::CancelOrder(cancel) => cancel.request_id,
    }
}

fn map_schedule_block(
    reason: Stage8bP1d1ScheduleBridgeBlockReason,
) -> Stage8bP1d1ExecutionEligibilityBlockReason {
    match reason {
        Stage8bP1d1ScheduleBridgeBlockReason::InstrumentMismatch => {
            Stage8bP1d1ExecutionEligibilityBlockReason::WrongInstrument
        }
        Stage8bP1d1ScheduleBridgeBlockReason::NotYetObserved => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleNotYetObserved
        }
        Stage8bP1d1ScheduleBridgeBlockReason::Expired => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleExpired
        }
        Stage8bP1d1ScheduleBridgeBlockReason::CandidateInFuture => {
            Stage8bP1d1ExecutionEligibilityBlockReason::CandidateObservedInFuture
        }
        Stage8bP1d1ScheduleBridgeBlockReason::CrossTradingDay => {
            Stage8bP1d1ExecutionEligibilityBlockReason::CrossTradingDay
        }
        Stage8bP1d1ScheduleBridgeBlockReason::ExecutionBarGap => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap
        }
        Stage8bP1d1ScheduleBridgeBlockReason::Rejected => {
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleRejected
        }
    }
}

fn derive_order_id(
    operational_identity_sha256: &str,
    request_id: StrategyRequestId,
    canonical_command_sha256: &str,
) -> BrokerOrderId {
    BrokerOrderId::new(format!(
        "P1D-O-{}",
        nul_separated_sha256(&[
            STAGE8B_P1D1_ORDER_ID_DOMAIN,
            operational_identity_sha256,
            &request_id.to_string(),
            canonical_command_sha256,
        ])
    ))
}

fn derive_trade_id(
    broker_order_id: &BrokerOrderId,
    execution_bar_semantic_id_sha256: &str,
) -> BrokerTradeId {
    BrokerTradeId::new(format!(
        "P1D-T-{}",
        nul_separated_sha256(&[
            STAGE8B_P1D1_TRADE_ID_DOMAIN,
            broker_order_id.as_str(),
            execution_bar_semantic_id_sha256,
            "1",
        ])
    ))
}

fn nul_separated_sha256(fields: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            hasher.update([0]);
        }
        hasher.update(field.as_bytes());
    }
    hex_digest(hasher.finalize())
}

#[allow(
    dead_code,
    reason = "production caller remains sealed with the P1-d1 entry seam"
)]
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(hasher.finalize())
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value != "0".repeat(64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_core::{
        BrokerAccountId, ClientOrderId, Exchange, HybridRuntimeAttribution, Market, OrderSide,
        PlaceOrder,
    };
    use chrono::TimeZone;
    use uuid::Uuid;

    fn instrument() -> InstrumentId {
        InstrumentId {
            symbol: "IMOEXF".to_string(),
            venue_symbol: Some("IMOEXF@RTSX".to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
        }
    }

    fn command(ttl_ms: Option<u64>) -> BrokerCommand {
        let request_id = StrategyRequestId::new(
            Uuid::parse_str("12345678-1234-5678-9234-567812345678").unwrap(),
        );
        let attribution = HybridRuntimeAttribution::parse_source_comment(
            "HYB|sid=hybrid_imoexf|c=cycle-p1d1|o=BO|r=ENTRY",
        )
        .unwrap();
        BrokerCommand::PlaceOrder(PlaceOrder {
            request_id,
            created_ts: Utc.with_ymd_and_hms(2026, 9, 3, 8, 50, 0).single().unwrap(),
            ttl_ms,
            account_id: BrokerAccountId::new("ACC_TEST_0001"),
            client_order_id: ClientOrderId::from_strategy_request(request_id),
            instrument: instrument(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            qty: Decimal::ONE,
            limit_price: None,
            time_in_force: TimeInForce::Day,
            comment: Some(attribution.internal_comment().to_string()),
        })
    }

    fn command_sha(command: &BrokerCommand) -> String {
        sha256_hex(&serde_json::to_vec(command).unwrap())
    }

    fn waiting(
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
        boundary: bool,
    ) -> Stage8bP1d1AwaitingExecutionBar {
        let command = command(None);
        let projection = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
            stage8b_p1d1_test_schedule_projection(
                instrument(),
                predecessor_close_ms.div_euclid(1_000),
                candidate_close_ms.div_euclid(1_000),
                boundary,
            );
        let hash = command_sha(&command);
        begin_stage8b_p1d1_market_wait(
            projection,
            command,
            "1".repeat(64),
            hash,
            "2".repeat(64),
            predecessor_close_ms,
        )
        .unwrap()
    }

    fn observation(
        command: &BrokerCommand,
        candidate_close_ms: i64,
    ) -> Stage8bP1d1ExecutionBarObservation {
        Stage8bP1d1ExecutionBarObservation {
            strategy_request_id: command_request_id(command),
            canonical_command_sha256: command_sha(command),
            predecessor_semantic_id_sha256: "2".repeat(64),
            bar: Stage8bP1d1CanonicalExecutionBar {
                operational_identity_sha256: "1".repeat(64),
                instrument: instrument(),
                semantic_id_sha256: "3".repeat(64),
                payload_sha256: "4".repeat(64),
                open_ts_utc_ms: candidate_close_ms - M10_MILLIS,
                close_ts_utc_ms: candidate_close_ms,
                open: Decimal::new(2_175, 1),
                timeframe_sec: 600,
                is_final: true,
                is_live: true,
            },
        }
    }

    fn observed_at(candidate_close_ms: i64) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(candidate_close_ms)
            .single()
            .unwrap()
    }

    fn eligible(
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
        boundary: bool,
    ) -> Stage8bP1d1ExecutionEligible {
        eligible_at(
            predecessor_close_ms,
            candidate_close_ms,
            boundary,
            candidate_close_ms,
        )
    }

    fn eligible_at(
        predecessor_close_ms: i64,
        candidate_close_ms: i64,
        boundary: bool,
        observed_at_ms: i64,
    ) -> Stage8bP1d1ExecutionEligible {
        let wait = waiting(predecessor_close_ms, candidate_close_ms, boundary);
        let command = wait.command.clone();
        match wait.observe_candidates(
            vec![observation(&command, candidate_close_ms)],
            observed_at(observed_at_ms),
        ) {
            Stage8bP1d1ExecutionObservation::Eligible(value) => *value,
            _ => panic!("fixture must be eligible"),
        }
    }

    #[test]
    fn p1d1_no_input_and_same_bar_never_mint_eligibility() {
        let predecessor = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let wait = waiting(predecessor, candidate, false);
        let wait = match wait.observe_candidates(Vec::new(), observed_at(candidate)) {
            Stage8bP1d1ExecutionObservation::NoInput(wait) => *wait,
            _ => panic!("empty observation must remain pending"),
        };
        let command = wait.command.clone();
        let blocked = match wait.observe_candidates(
            vec![observation(&command, predecessor)],
            observed_at(candidate),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(blocked) => blocked,
            _ => panic!("same-bar execution must be blocked"),
        };
        assert_eq!(
            blocked.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::SameBar
        );
    }

    #[test]
    fn p1d1_contiguous_and_clearing_boundary_are_the_only_positive_paths() {
        let predecessor = 1_788_422_400_000;
        assert!(matches!(
            waiting(predecessor, predecessor + M10_MILLIS, false).observe_candidates(
                vec![observation(&command(None), predecessor + M10_MILLIS)],
                observed_at(predecessor + M10_MILLIS),
            ),
            Stage8bP1d1ExecutionObservation::Eligible(_)
        ));
        assert!(matches!(
            waiting(predecessor, predecessor + 3 * M10_MILLIS, true).observe_candidates(
                vec![observation(&command(None), predecessor + 3 * M10_MILLIS)],
                observed_at(predecessor + 3 * M10_MILLIS),
            ),
            Stage8bP1d1ExecutionObservation::Eligible(_)
        ));
        let blocked = match waiting(predecessor, predecessor + 3 * M10_MILLIS, false)
            .observe_candidates(
                vec![observation(&command(None), predecessor + 3 * M10_MILLIS)],
                observed_at(predecessor + 3 * M10_MILLIS),
            ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("skipped tradable M10 must be a gap"),
        };
        assert_eq!(
            blocked.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::ExecutionBarGap
        );
    }

    #[test]
    fn p1d1_wrong_binding_history_and_multiple_candidates_fail_closed() {
        let predecessor = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let cases = [
            (
                {
                    let mut value = observation(&command(None), candidate);
                    value.strategy_request_id = StrategyRequestId::new(
                        Uuid::parse_str("87654321-4321-8765-9321-876543218765").unwrap(),
                    );
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::WrongRequest,
            ),
            (
                {
                    let mut value = observation(&command(None), candidate);
                    value.canonical_command_sha256 = "5".repeat(64);
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::WrongCommand,
            ),
            (
                {
                    let mut value = observation(&command(None), candidate);
                    value.predecessor_semantic_id_sha256 = "6".repeat(64);
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::WrongPredecessor,
            ),
            (
                {
                    let mut value = observation(&command(None), candidate);
                    value.bar.is_live = false;
                    value
                },
                Stage8bP1d1ExecutionEligibilityBlockReason::HistoryOrWarmup,
            ),
        ];
        for (observation, expected) in cases {
            let blocked = match waiting(predecessor, candidate, false)
                .observe_candidates(vec![observation], observed_at(candidate))
            {
                Stage8bP1d1ExecutionObservation::Blocked(value) => value,
                _ => panic!("mutation must block"),
            };
            assert_eq!(blocked.reason(), expected);
        }

        let command = command(None);
        let first = observation(&command, candidate);
        let second = first.clone();
        let blocked = match waiting(predecessor, candidate, false)
            .observe_candidates(vec![first, second], observed_at(candidate))
        {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("multiple candidates must block"),
        };
        assert_eq!(
            blocked.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::MultipleCandidates
        );
    }

    #[test]
    fn p1d1_ttl_is_fail_closed_before_wait_authority() {
        let command = command(Some(30_000));
        let predecessor: i64 = 1_788_422_400_000;
        let projection = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
            stage8b_p1d1_test_schedule_projection(
                instrument(),
                predecessor.div_euclid(1_000),
                (predecessor + M10_MILLIS).div_euclid(1_000),
                false,
            );
        let hash = command_sha(&command);
        assert!(matches!(
            begin_stage8b_p1d1_market_wait(
                projection,
                command,
                "1".repeat(64),
                hash,
                "2".repeat(64),
                predecessor,
            ),
            Err(Stage8bP1d1ProviderError::TtlForbidden)
        ));
    }

    #[test]
    fn p1d1_schedule_freshness_future_and_cross_day_fail_closed() {
        let predecessor: i64 = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let command = command(None);

        let not_yet = match waiting(predecessor, candidate, false).observe_candidates(
            vec![observation(&command, candidate)],
            observed_at(predecessor - 1_000),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("schedule used before observation must block"),
        };
        assert_eq!(
            not_yet.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleNotYetObserved
        );

        let future = match waiting(predecessor, candidate, false).observe_candidates(
            vec![observation(&command, candidate)],
            observed_at(candidate - 1_000),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("future candidate must block"),
        };
        assert_eq!(
            future.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::CandidateObservedInFuture
        );

        let expired = match waiting(predecessor, candidate, false).observe_candidates(
            vec![observation(&command, candidate)],
            observed_at(candidate + 3_601_000),
        ) {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("expired schedule must block"),
        };
        assert_eq!(
            expired.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::ScheduleExpired
        );

        let midnight = Utc
            .with_ymd_and_hms(2026, 9, 4, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        let prior_day = midnight - M10_MILLIS;
        let cross_day = match waiting(prior_day, midnight, false)
            .observe_candidates(vec![observation(&command, midnight)], observed_at(midnight))
        {
            Stage8bP1d1ExecutionObservation::Blocked(value) => value,
            _ => panic!("cross-day sequence must block"),
        };
        assert_eq!(
            cross_day.reason(),
            Stage8bP1d1ExecutionEligibilityBlockReason::CrossTradingDay
        );
    }

    #[test]
    fn p1d1_market_result_is_exact_and_deterministic_after_dispatch() {
        let predecessor = 1_788_422_400_000;
        let candidate = predecessor + M10_MILLIS;
        let first = eligible(predecessor, candidate, false);
        let second = eligible_at(predecessor, candidate, false, candidate + 30_000);
        let first_receipt =
            crate::stage6d_live_core::stage8b_p1d1_test_dispatch_receipt(&first.command);
        let second_receipt =
            crate::stage6d_live_core::stage8b_p1d1_test_dispatch_receipt(&second.command);
        let first = bind_stage8b_p1d1_market_dispatch(first, first_receipt)
            .unwrap()
            .execute();
        let second = bind_stage8b_p1d1_market_dispatch(second, second_receipt)
            .unwrap()
            .execute();
        assert_eq!(first.fill_price(), Decimal::new(2_175, 1));
        assert_eq!(first.fill_source_ts_utc_ms(), candidate - M10_MILLIS);
        assert_eq!(first.fill_qty(), Decimal::ONE);
        assert_eq!(first.broker_order_id(), second.broker_order_id());
        assert_eq!(first.broker_trade_id(), second.broker_trade_id());
        assert_eq!(first.broker_order_id().as_str().len(), 70);
        assert_eq!(first.broker_trade_id().as_str().len(), 70);
        assert!(!first.feedback_application_allowed());
        assert!(!first.ack_allowed());
        assert!(!first.source_m10_xack_allowed());
    }
}
