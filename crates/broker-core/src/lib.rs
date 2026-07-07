//! Broker-neutral contracts shared by MOEX broker adapters and strategy bridges.
//!
//! This crate deliberately contains no Finam-, T-Bank-, or Alor-specific API
//! shapes. Adapters translate broker payloads into these contracts.

pub mod account;
pub mod bar_aggregation;
pub mod bar_finalizer;
pub mod broker;
pub mod command;
pub mod envelope;
pub mod event;
pub mod ids;
pub mod instrument;
pub mod market_data_lifecycle;
pub mod market_data_recovery;
pub mod observability;
pub mod operational_config;
pub mod operational_snapshot;
pub mod order;
pub mod order_path;
pub mod paper;
pub mod parity;
pub mod readiness;
pub mod runtime_host;
pub mod runtime_state;
pub mod subscription;
pub mod time;

pub use account::{AccountId, PortfolioSnapshot, Position};
pub use bar_aggregation::{
    BarAggregationAction, BarAggregationRejectReason, CanonicalBarAggregator,
};
pub use bar_finalizer::{
    ClosedBarFinalizer, ClosedBarFinalizerAction, ClosedBarFinalizerActionKind, ClosedBarStreamKey,
};
pub use broker::BrokerKind;
pub use command::{
    BrokerCommand, CancelOrder, CommandAck, CommandAckReason, CommandAckReasonCode, PlaceOrder,
};
pub use envelope::{Envelope, MessageType, SCHEMA_VERSION};
pub use event::{BrokerEvent, MarketDataEvent, MarketDataSourceKind};
pub use ids::{
    deserialize_broker_order_id_legacy_numeric_or_string,
    deserialize_option_broker_order_id_legacy_numeric_or_string,
    deserialize_vec_broker_order_id_legacy_numeric_or_string, BrokerAccountId, BrokerOrderId,
    BrokerOrderIdEncoding, BrokerOrderIdImportError, BrokerTradeId, ClientOrderId,
    ClientOrderIdError, StrategyRequestId, BROKER_ORDER_ID_ENCODING, CLIENT_ORDER_ID_MAX_LEN,
    LEGACY_ALOR_NUMERIC_ORDER_ID_IMPORT, RUNTIME_STATE_SCHEMA_VERSION_V2,
};
pub use instrument::{
    BrokerSymbol, Exchange, Instrument, InstrumentId, InstrumentMapEntry, InternalSymbol, Market,
    Money, Price, Quantity,
};
pub use market_data_lifecycle::{
    evaluate_market_data_lifecycle, BrokerMarketDataLifecycleInput,
    BrokerMarketDataLifecycleSnapshot, MarketDataLifecycleBlocker, MarketDataLifecyclePhase,
};
pub use market_data_recovery::{
    evaluate_market_data_recovery, plan_market_data_recovery, MarketDataRecoveryBlocker,
    MarketDataRecoveryInput, MarketDataRecoveryMode, MarketDataRecoveryPhase,
    MarketDataRecoveryPlan, MarketDataRecoveryPlanInput, MarketDataRecoveryReport,
};
pub use observability::{
    live_required_channel_kinds, required_channel_kinds, BrokerConsumerGroupSnapshot,
    BrokerObservabilityBlocker, BrokerObservabilityChannel, BrokerObservabilityChannelKind,
    BrokerObservabilityContourRole, BrokerObservabilityContract, BrokerObservabilityOwner,
    BrokerObservabilityReadinessReport, OBSERVABILITY_SCHEMA_VERSION,
};
pub use operational_config::{
    BrokerCanonicalPreflightBlock, BrokerCanonicalPreflightDecision, BrokerCapabilityMatrix,
    BrokerFeedFreshness, BrokerFreshnessConfig, BrokerLifecycleConfig, BrokerLiveEntryBlock,
    BrokerLiveEntryDecision, BrokerLiveEntryScope, BrokerMarketSessionState,
    BrokerOperationalConfig, BrokerOrderIntentKind, BrokerPlainMicroStopOrderWaiverPolicy,
    BrokerReadinessSnapshot, BrokerRiskLimitConfig, BrokerScopeConfig, BrokerStopOrderReadiness,
    BrokerStopOrderWaiverDecision, BrokerStopOrderWaiverRejection, BrokerStopOrderWaiverSource,
    BrokerTimeoutConfig,
};
pub use operational_snapshot::{
    instrument_identity_matches, instrument_spec_identity_matches, BrokerCashSnapshot,
    BrokerInstrumentSpec, BrokerMarginSufficiency, BrokerOrderLifecycle,
    BrokerOrderMarginSufficiency, BrokerOrderOrphanReason, BrokerOrderQuantityTruth,
    BrokerOrderSnapshot, BrokerPositionSnapshot, BrokerRequiredMargin, BrokerRequiredMarginFailure,
    BrokerTradeSnapshot, BrokerTruthInstrumentSummary, BrokerTruthSnapshot,
};
pub use order::{
    Order, OrderId, OrderSide, OrderStatus, OrderType, RedactedValueFingerprint, StopKind,
    TimeInForce, Trade, TradeId,
};
pub use order_path::{
    inspect_sqlite_runtime_directory, CancelPreflightApproval, CancelPreflightDecision,
    CommentPolicyMode, DryOrderRateLimit, DryOrderRateLimitError, DryOrderRateWindow,
    DryOrderRateWindowDecision, DryOrderRateWindowError, InMemoryOrderPathStore,
    JsonFileOrderPathStore, OperatorArm, OperatorDisarmDecision, OperatorDisarmSignal,
    OrderPathCommandKind, OrderPathErrorKind, OrderPathEvent, OrderPathReconciliationSource,
    OrderPathRecord, OrderPathState, OrderPathStore, OrderPathStoreError, OrderPathTransitionError,
    OrderPreflightContext, OrderPreflightError, OrderPreflightPolicy, OrderReferencePrice,
    OutgoingCommentError, OutgoingCommentIntent, OutgoingOrderComment, OutgoingOrderCommentPolicy,
    PreflightApprovedCancelOrder, PreflightApprovedPlaceOrder, SqliteOrderPathReadStore,
    SqliteOrderPathRedactedRecord, SqliteOrderPathStore, SqliteOrderPathTransitionAudit,
    SqliteRuntimeDirectoryIssue, SqliteWriterLockMetadata,
};
pub use paper::{
    f64_to_price, PaperAck, PaperAckKind, PaperExecutionMode, PaperFillPolicy,
    PaperHybridIntradayOracleSeed, PaperHybridIntradayRuntimeStateProjection,
    PaperHybridStrategyShadowConfig, PaperHybridStrategyShadowState, PaperIntent, PaperIntentKind,
    PaperLedgerExecutionOutcome, PaperLedgerExecutorConfig, PaperLedgerExecutorError,
    PaperLedgerInvariantError, PaperLedgerSnapshot, PaperOrder, PaperOrderId, PaperOrderStatus,
    PaperPosition, PaperRuntimeAdapter, PaperRuntimeAdapterConfig, PaperRuntimeAdapterError,
    PaperRuntimeAdapterLoop, PaperRuntimeAdapterLoopError, PaperRuntimeAdapterLoopOutcome,
    PaperRuntimeAdapterOutcome, PaperRuntimeBarPublishOutcome, PaperRuntimeBarPublishRejectReason,
    PaperRuntimeBarPublisher, PaperRuntimeBarPublisherConfig, PaperRuntimeInMemorySink,
    PaperRuntimePublishPayload, PaperRuntimePublishRecord, PaperRuntimePublishSink,
    PaperRuntimePublishSinkError, PaperRuntimeState, PaperRuntimeStreams, PaperSafetyBoundary,
    PaperTrade, PaperTradeId, RiskGatePaperLedgerRecord, RiskGatePaperState, RuntimeBarInput,
    RuntimeBarOrigin, RuntimeDecisionId, RuntimeDecisionRecord, RuntimeSuppressionReason,
    RuntimeSuppressionRecord,
};
pub use parity::{
    compare_broker_truth_for_instrument, compare_final_bars_for_instrument, BrokerBarParityReport,
    BrokerParityIssue, BrokerParityIssueKind, BrokerTruthParityReport,
};
pub use readiness::{BrokerReadiness, ReadinessPhase, ReadinessReason};
pub use runtime_host::{
    evaluate_runtime_live_guard, validate_runtime_lifecycle_sequence, RuntimeCommandPrepared,
    RuntimeEventClock, RuntimeHostBlockedIntentDisposition, RuntimeHostBootstrapSnapshot,
    RuntimeHostContract, RuntimeHostLifecycleIssue, RuntimeHostLifecyclePlan,
    RuntimeHostLifecycleStep, RuntimeHostLiveGuardDecision, RuntimeHostLiveGuardInput,
    RuntimeIntentBlockEvent, RuntimeIntentClass, RuntimeStrategyContext,
};
pub use runtime_state::{
    RuntimeBootstrapSnapshotDto, RuntimeCommandAckDto, RuntimeOrderEvent,
    RuntimeStateReadinessBlocker, RuntimeStateReadinessBlockerKind, RuntimeStateSnapshot,
    RuntimeStateValidationError, RuntimeTradeEvent, ValidatedRuntimeBootstrapSnapshotDto,
    ValidatedRuntimeStateSnapshot,
};
pub use subscription::{SubscriptionIntent, SubscriptionKind, SubscriptionState};
