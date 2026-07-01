//! Broker-neutral contracts shared by MOEX broker adapters and strategy bridges.
//!
//! This crate deliberately contains no Finam-, T-Bank-, or Alor-specific API
//! shapes. Adapters translate broker payloads into these contracts.

pub mod account;
pub mod broker;
pub mod command;
pub mod envelope;
pub mod event;
pub mod ids;
pub mod instrument;
pub mod order;
pub mod order_path;
pub mod readiness;
pub mod subscription;
pub mod time;

pub use account::{AccountId, PortfolioSnapshot, Position};
pub use broker::BrokerKind;
pub use command::{
    BrokerCommand, CancelOrder, CommandAck, CommandAckReason, CommandAckReasonCode, PlaceOrder,
};
pub use envelope::{Envelope, MessageType, SCHEMA_VERSION};
pub use event::{BrokerEvent, MarketDataEvent, MarketDataSourceKind};
pub use ids::{
    BrokerAccountId, BrokerOrderId, BrokerTradeId, ClientOrderId, ClientOrderIdError,
    StrategyRequestId, CLIENT_ORDER_ID_MAX_LEN,
};
pub use instrument::{
    BrokerSymbol, Exchange, Instrument, InstrumentId, InstrumentMapEntry, InternalSymbol, Market,
    Money, Price, Quantity,
};
pub use order::{
    Order, OrderId, OrderSide, OrderStatus, OrderType, RedactedValueFingerprint, StopKind,
    TimeInForce, Trade, TradeId,
};
pub use order_path::{
    CancelPreflightApproval, CancelPreflightDecision, CommentPolicyMode, DryOrderRateLimit,
    DryOrderRateLimitError, DryOrderRateWindow, DryOrderRateWindowDecision,
    DryOrderRateWindowError, InMemoryOrderPathStore, JsonFileOrderPathStore, OperatorArm,
    OperatorDisarmDecision, OperatorDisarmSignal, OrderPathCommandKind, OrderPathErrorKind,
    OrderPathEvent, OrderPathReconciliationSource, OrderPathRecord, OrderPathState, OrderPathStore,
    OrderPathStoreError, OrderPathTransitionError, OrderPreflightContext, OrderPreflightError,
    OrderPreflightPolicy, OrderReferencePrice, OutgoingCommentError, OutgoingCommentIntent,
    OutgoingOrderComment, OutgoingOrderCommentPolicy, PreflightApprovedCancelOrder,
    PreflightApprovedPlaceOrder, SqliteOrderPathReadStore, SqliteOrderPathRedactedRecord,
    SqliteOrderPathStore, SqliteOrderPathTransitionAudit, SqliteWriterLockMetadata,
};
pub use readiness::{BrokerReadiness, ReadinessPhase, ReadinessReason};
pub use subscription::{SubscriptionIntent, SubscriptionKind, SubscriptionState};
